use mongodb::{
    bson::{doc, Document},
    options::ClientOptions,
    Client,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tauri::State;
use crate::ssh::TunnelPool;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub uri: Option<String>,
    pub host: Option<String>,
    pub port: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub options: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub success: bool,
    pub data: Option<Vec<serde_json::Value>>,
    pub error: Option<String>,
    pub rows: u64,
    pub elapsed_ms: u128,
}

// ─── Extra result types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CollectionStats {
    pub ns: String,
    pub count: i64,
    pub storage_size: i64,
    pub avg_obj_size: i64,
    pub total_index_size: i64,
    pub index_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SchemaField {
    pub path: String,
    pub types: Vec<String>,
    pub count: usize,
    pub presence: f64,
}

#[derive(Debug, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub keys: serde_json::Value,
    pub unique: bool,
    pub sparse: bool,
}

// ─── Connection pool (shared state) ──────────────────────────────────────────

pub struct MongoPool(pub Arc<Mutex<HashMap<String, Client>>>);

impl MongoPool {
    pub fn new() -> Self {
        MongoPool(Arc::new(Mutex::new(HashMap::new())))
    }
}

// ─── URI builder ─────────────────────────────────────────────────────────────

fn build_uri(conn: &ConnectionConfig, tunnel_port: Option<u16>) -> String {
    // When a tunnel is active, connect to localhost:<tunnel_port> regardless of conn URI/host.
    let (host, port) = if let Some(tp) = tunnel_port {
        ("127.0.0.1".to_string(), tp.to_string())
    } else {
        let h = conn.host.as_deref().unwrap_or("localhost").to_string();
        let p = conn.port.as_deref().unwrap_or("27017").to_string();
        (h, p)
    };

    // When tunnel is active, ignore the original URI (it routes to the remote host) and
    // build a new one pointing at the local tunnel endpoint.
    if tunnel_port.is_none() {
        if let Some(uri) = &conn.uri {
            if !uri.is_empty() {
                return uri.clone();
            }
        }
    }

    let options = conn.options.as_deref().unwrap_or("");
    match (&conn.user, &conn.password) {
        (Some(u), Some(p)) if !u.is_empty() => {
            let opts = if options.is_empty() {
                "authSource=admin".to_string()
            } else {
                format!("{}&authSource=admin", options)
            };
            format!("mongodb://{}:{}@{}:{}/admin?{}", u, p, host, port, opts)
        }
        _ => {
            if options.is_empty() {
                format!("mongodb://{}:{}", host, port)
            } else {
                format!("mongodb://{}:{}?{}", host, port, options)
            }
        }
    }
}

// ─── Get or create client ─────────────────────────────────────────────────────

async fn get_client(
    pool: &MongoPool,
    conn: &ConnectionConfig,
    tunnels: &TunnelPool,
) -> Result<Client, String> {
    // Check pool first (read lock)
    {
        let pool_map = pool.0.lock().map_err(|e| e.to_string())?;
        if let Some(client) = pool_map.get(&conn.id) {
            return Ok(client.clone());
        }
    }

    // If SSH tunnel is configured, open it first and get the local port.
    let tunnel_port = if conn.ssh_host.as_deref().unwrap_or("").is_empty() {
        None
    } else {
        let ssh_host = conn.ssh_host.clone().unwrap_or_default();
        let ssh_port = conn.ssh_port.clone().unwrap_or_default();
        let ssh_user = conn.ssh_user.clone().unwrap_or_default();
        let ssh_key  = conn.ssh_key_path.clone().unwrap_or_default();
        // Determine remote MongoDB host/port (from URI or params)
        let (remote_host, remote_port) = if let Some(uri) = &conn.uri {
            // Parse host:port from mongodb://[user:pass@]host:port/...
            let stripped = uri
                .trim_start_matches("mongodb+srv://")
                .trim_start_matches("mongodb://");
            let hp = stripped.split('/').next().unwrap_or("");
            // strip credentials
            let hp = hp.split('@').next_back().unwrap_or(hp);
            let (h, p) = hp.split_once(':').unwrap_or((hp, "27017"));
            (h.to_string(), p.to_string())
        } else {
            (
                conn.host.clone().unwrap_or_else(|| "localhost".to_string()),
                conn.port.clone().unwrap_or_else(|| "27017".to_string()),
            )
        };
        let port = crate::ssh::open_tunnel_internal(
            &conn.id, &ssh_host, &ssh_port, &ssh_user, &ssh_key,
            &remote_host, &remote_port, tunnels,
        )?;
        Some(port)
    };

    // Create new client
    let uri = build_uri(conn, tunnel_port);
    let opts = ClientOptions::parse(&uri)
        .await
        .map_err(|e| format!("Invalid URI: {}", e))?;
    let client = Client::with_options(opts).map_err(|e| e.to_string())?;

    // Store in pool
    let mut pool_map = pool.0.lock().map_err(|e| e.to_string())?;
    pool_map.insert(conn.id.clone(), client.clone());
    Ok(client)
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_connection(
    conn: ConnectionConfig,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<TestResult, String> {
    match get_client(&pool, &conn, &tunnels).await {
        Ok(client) => {
            let db = client.database("admin");
            match db.run_command(doc! { "ping": 1 }).await {
                Ok(_) => Ok(TestResult {
                    success: true,
                    message: "Conexión exitosa".to_string(),
                }),
                Err(e) => Ok(TestResult {
                    success: false,
                    message: format!("Error: {}", e),
                }),
            }
        }
        Err(e) => Ok(TestResult {
            success: false,
            message: format!("Error: {}", e),
        }),
    }
}

#[tauri::command]
pub async fn list_databases(
    conn: ConnectionConfig,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<Vec<String>, String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    let dbs = client
        .list_database_names()
        .await
        .map_err(|e| e.to_string())?;
    let mut sorted = dbs;
    sorted.sort();
    Ok(sorted)
}

#[tauri::command]
pub async fn list_collections(
    conn: ConnectionConfig,
    db_name: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<Vec<String>, String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    let db = client.database(&db_name);
    let cols = db
        .list_collection_names()
        .await
        .map_err(|e| e.to_string())?;
    let mut sorted = cols;
    sorted.sort();
    Ok(sorted)
}

#[tauri::command]
pub async fn disconnect(
    conn_id: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<(), String> {
    pool.0.lock().map_err(|e| e.to_string())?.remove(&conn_id);
    if let Some(mut c) = tunnels.0.lock().map_err(|e| e.to_string())?.remove(&conn_id) { let _ = c.kill(); }
    Ok(())
}

#[tauri::command]
pub async fn check_connection(
    conn_id: String,
    pool: State<'_, MongoPool>,
) -> Result<bool, String> {
    let client = {
        let pool_map = pool.0.lock().map_err(|e| e.to_string())?;
        pool_map.get(&conn_id).cloned()
    };
    match client {
        Some(c) => match c.database("admin").run_command(doc! { "ping": 1 }).await {
            Ok(_) => Ok(true),
            Err(_) => {
                // Dead connection — remove from pool so next call creates a fresh one
                pool.0.lock().map_err(|e| e.to_string())?.remove(&conn_id);
                Ok(false)
            }
        },
        None => Ok(false),
    }
}

#[tauri::command]
pub async fn create_collection(
    conn: ConnectionConfig,
    db_name: String,
    collection: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<(), String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    client
        .database(&db_name)
        .create_collection(&collection)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn drop_collection_cmd(
    conn: ConnectionConfig,
    db_name: String,
    collection: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<(), String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    client
        .database(&db_name)
        .collection::<mongodb::bson::Document>(&collection)
        .drop()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn drop_database_cmd(
    conn: ConnectionConfig,
    db_name: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<(), String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    client
        .database(&db_name)
        .drop()
        .await
        .map_err(|e| e.to_string())
}

// ─── Query execution ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn execute_query(
    conn: ConnectionConfig,
    db_name: String,
    query: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<QueryResult, String> {
    let start = std::time::Instant::now();
    let client = get_client(&pool, &conn, &tunnels).await?;
    let db = client.database(&db_name);

    let result = run_query(&db, &query).await;
    let elapsed_ms = start.elapsed().as_millis();

    match result {
        Ok(docs) => {
            let rows = docs.len() as u64;
            Ok(QueryResult {
                success: true,
                data: Some(docs),
                error: None,
                rows,
                elapsed_ms,
            })
        }
        Err(e) => Ok(QueryResult {
            success: false,
            data: None,
            error: Some(e),
            rows: 0,
            elapsed_ms,
        }),
    }
}

/// Parse and execute a MongoDB shell-style query string.
/// Supports: find, findOne, aggregate, count/countDocuments, insertOne, insertMany,
///           updateOne, updateMany, deleteOne, deleteMany, drop
async fn run_query(db: &mongodb::Database, query: &str) -> Result<Vec<serde_json::Value>, String> {
    let query = query.trim().trim_end_matches(';').trim();

    // Extract collection name: db.getCollection("x") or db.x
    let (collection_name, operation_str) = parse_collection_and_op(query)?;
    let col = db.collection::<Document>(&collection_name);

    let op = operation_str.trim();

    // ── find ──────────────────────────────────────────────────────────────────
    if op.starts_with("find(") {
        let args = extract_parens_content(op, "find")?;
        let (filter, opts_doc) = parse_two_docs(args)?;
        let mut find = col.find(filter);
        if let Some(proj) = opts_doc {
            // projection as second arg
            use mongodb::options::FindOptions;
            let mut fopts = FindOptions::default();
            fopts.projection = Some(proj);
            find = find.with_options(fopts);
        }
        // chain modifiers: .sort({}).limit(n).skip(n)
        use mongodb::options::FindOptions;
        let mut fopts = FindOptions::default();
        if let Some(ss) = op.find(".sort(") { let r = &op[ss+6..]; if let Ok(e) = find_first_doc_end(r) { if let Ok(d) = parse_doc(r[..e].trim()) { fopts.sort = Some(d); } } }
        if let Some(ls) = op.find(".limit(") { let r = &op[ls+7..]; let e = r.find(')').unwrap_or(r.len()); if let Ok(n) = r[..e].trim().parse::<i64>() { fopts.limit = Some(n); } }
        if let Some(sk) = op.find(".skip(") { let r = &op[sk+6..]; let e = r.find(')').unwrap_or(r.len()); if let Ok(n) = r[..e].trim().parse::<u64>() { fopts.skip = Some(n); } }
        let find = find.with_options(fopts);
        let mut cursor: mongodb::Cursor<Document> = find.await.map_err(|e| e.to_string())?;
        return collect_cursor(&mut cursor).await;
    }

    // ── findOne ───────────────────────────────────────────────────────────────
    if op.starts_with("findOne(") {
        let args = extract_parens_content(op, "findOne")?;
        let (filter, _) = parse_two_docs(args)?;
        let doc = col
            .find_one(filter)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(doc
            .map(|d| vec![doc_to_json(d)])
            .unwrap_or_default());
    }

    // ── aggregate ─────────────────────────────────────────────────────────────
    if op.starts_with("aggregate(") {
        let args = extract_parens_content(op, "aggregate")?;
        let pipeline = parse_pipeline(args)?;
        let mut cursor = col.aggregate(pipeline).await.map_err(|e| e.to_string())?;
        return collect_cursor(&mut cursor).await;
    }

    // ── countDocuments / count ────────────────────────────────────────────────
    if op.starts_with("countDocuments(") || op.starts_with("count(") {
        let fname = if op.starts_with("countDocuments(") { "countDocuments" } else { "count" };
        let args = extract_parens_content(op, fname)?;
        let (filter, _) = parse_two_docs(args)?;
        let n = col.count_documents(filter).await.map_err(|e| e.to_string())?;
        return Ok(vec![serde_json::json!({ "count": n })]);
    }

    // ── insertOne ─────────────────────────────────────────────────────────────
    if op.starts_with("insertOne(") {
        let args = extract_parens_content(op, "insertOne")?;
        let doc: Document = parse_doc(args)?;
        let res = col.insert_one(doc).await.map_err(|e| e.to_string())?;
        let id = res.inserted_id.to_string();
        return Ok(vec![serde_json::json!({ "insertedId": id, "acknowledged": true })]);
    }

    // ── insertMany ────────────────────────────────────────────────────────────
    if op.starts_with("insertMany(") {
        let args = extract_parens_content(op, "insertMany")?;
        let docs = parse_pipeline(args)?;
        let res = col.insert_many(docs).await.map_err(|e| e.to_string())?;
        let ids: Vec<String> = res.inserted_ids.values().map(|v| v.to_string()).collect();
        return Ok(vec![serde_json::json!({ "insertedCount": ids.len(), "insertedIds": ids })]);
    }

    // ── updateOne / updateMany ────────────────────────────────────────────────
    if op.starts_with("updateOne(") || op.starts_with("updateMany(") {
        let fname = if op.starts_with("updateOne(") { "updateOne" } else { "updateMany" };
        let args = extract_parens_content(op, fname)?;
        let (filter, update) = parse_two_docs(args)?;
        let update = update.ok_or("updateOne/updateMany requires a second argument (update)")?;
        let res = if fname == "updateOne" {
            let r = col.update_one(filter, update).await.map_err(|e| e.to_string())?;
            serde_json::json!({ "matchedCount": r.matched_count, "modifiedCount": r.modified_count })
        } else {
            let r = col.update_many(filter, update).await.map_err(|e| e.to_string())?;
            serde_json::json!({ "matchedCount": r.matched_count, "modifiedCount": r.modified_count })
        };
        return Ok(vec![res]);
    }

    // ── deleteOne / deleteMany ────────────────────────────────────────────────
    if op.starts_with("deleteOne(") || op.starts_with("deleteMany(") {
        let fname = if op.starts_with("deleteOne(") { "deleteOne" } else { "deleteMany" };
        let args = extract_parens_content(op, fname)?;
        let (filter, _) = parse_two_docs(args)?;
        let n = if fname == "deleteOne" {
            col.delete_one(filter).await.map_err(|e| e.to_string())?.deleted_count
        } else {
            col.delete_many(filter).await.map_err(|e| e.to_string())?.deleted_count
        };
        return Ok(vec![serde_json::json!({ "deletedCount": n })]);
    }

    // ── drop ──────────────────────────────────────────────────────────────────
    if op.starts_with("drop(") {
        col.drop().await.map_err(|e| e.to_string())?;
        return Ok(vec![serde_json::json!({ "dropped": true })]);
    }

    Err(format!("Operación no reconocida: '{}'", op))
}

// ─── Query parsing helpers ────────────────────────────────────────────────────

fn parse_collection_and_op(query: &str) -> Result<(String, String), String> {
    // db.getCollection("name").op(...) or db.name.op(...)
    if let Some(rest) = query.strip_prefix("db.getCollection(") {
        let end = rest.find(')').ok_or("getCollection: missing closing paren")?;
        let raw = rest[..end].trim().trim_matches('"').trim_matches('\'');
        let op_start = end + 1; // after ')'
        let rest2 = rest[op_start..].trim_start_matches('.').to_string();
        return Ok((raw.to_string(), rest2));
    }
    if let Some(rest) = query.strip_prefix("db.") {
        let dot = rest.find('.').ok_or("Expected db.collection.operation")?;
        let col = rest[..dot].to_string();
        let op = rest[dot + 1..].to_string();
        return Ok((col, op));
    }
    Err("Query must start with db.collection or db.getCollection(\"...\")".to_string())
}

fn extract_parens_content<'a>(op: &'a str, name: &str) -> Result<&'a str, String> {
    let prefix = format!("{}(", name);
    let inner = op
        .strip_prefix(prefix.as_str())
        .ok_or(format!("Expected {}", prefix))?;
    // Find matching closing paren (handle nesting)
    let mut depth = 1usize;
    let mut end = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    Ok(inner[..end].trim())
}

// ─── EJSON → BSON helpers ────────────────────────────────────────────────────

/// Gregorian date → days since Unix epoch (inverse of civil_from_days).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u32;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

/// Parse ISO 8601 string → milliseconds since Unix epoch.
/// Accepts: "2024-01-15", "2024-01-15T10:30:00Z", "2024-01-15T10:30:00.123Z"
fn iso_str_to_millis(s: &str) -> Option<i64> {
    let s = s.trim_end_matches('Z');
    let (date_part, time_part) = s.split_once('T').unwrap_or((s, "00:00:00"));
    let dp: Vec<&str> = date_part.split('-').collect();
    if dp.len() != 3 { return None; }
    let y: i64 = dp[0].parse().ok()?;
    let mo: u32 = dp[1].parse().ok()?;
    let d: u32  = dp[2].parse().ok()?;
    let tp: Vec<&str> = time_part.split(':').collect();
    let hh: i64 = tp.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let mm: i64 = tp.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let ss_frac = tp.get(2).unwrap_or(&"0");
    let (ss_s, fr) = ss_frac.split_once('.').unwrap_or((ss_frac, "0"));
    let ss: i64   = ss_s.parse().ok().unwrap_or(0);
    let fr_len    = fr.len().min(3);
    let frac: i64 = fr[..fr_len].parse().ok().unwrap_or(0) * 10i64.pow(3 - fr_len as u32);
    let days = days_from_civil(y, mo, d);
    Some(days * 86_400_000 + hh * 3_600_000 + mm * 60_000 + ss * 1000 + frac)
}

/// Recursively convert a JSON value (possibly containing EJSON patterns) to BSON.
/// Handles $oid → ObjectId, $date → DateTime, $regex → Regex.
fn ejson_value_to_bson(val: serde_json::Value) -> mongodb::bson::Bson {
    use mongodb::bson::{Bson, Document};
    match val {
        serde_json::Value::Null    => Bson::Null,
        serde_json::Value::Bool(b) => Bson::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Bson::Int64(i) }
            else { Bson::Double(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s) => {
            // Detect BSON wrapper patterns inside quoted strings (e.g., from copy-paste)
            // ISODate("...") or Date("...")
            if let Some(inner) = s.strip_prefix("ISODate(\"").and_then(|r| r.strip_suffix("\")"))
                .or_else(|| s.strip_prefix("Date(\"").and_then(|r| r.strip_suffix("\")")))
            {
                if let Some(ms) = iso_str_to_millis(inner) {
                    return Bson::DateTime(mongodb::bson::DateTime::from_millis(ms));
                }
            }
            // ObjectId("...")
            if let Some(inner) = s.strip_prefix("ObjectId(\"").and_then(|r| r.strip_suffix("\")")) {
                if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(inner) {
                    return Bson::ObjectId(oid);
                }
            }
            // NumberDecimal("...")
            if let Some(inner) = s.strip_prefix("NumberDecimal(\"").and_then(|r| r.strip_suffix("\")")) {
                if let Ok(d) = inner.parse::<f64>() {
                    return Bson::Double(d);
                }
            }
            // NumberLong(...)
            if let Some(inner) = s.strip_prefix("NumberLong(").and_then(|r| r.strip_suffix(")")) {
                let inner = inner.trim_matches('"');
                if let Ok(n) = inner.parse::<i64>() {
                    return Bson::Int64(n);
                }
            }
            Bson::String(s)
        }
        serde_json::Value::Array(arr) => {
            Bson::Array(arr.into_iter().map(ejson_value_to_bson).collect())
        }
        serde_json::Value::Object(map) => {
            // Regex: {"$regex":"pat"} or {"$regex":"pat","$options":"flags"}
            if let Some(pattern) = map.get("$regex").and_then(|v| v.as_str()) {
                let options = map.get("$options").and_then(|v| v.as_str()).unwrap_or("").to_string();
                return Bson::RegularExpression(mongodb::bson::Regex {
                    pattern: pattern.to_string(),
                    options,
                });
            }
            // Single-key EJSON patterns
            if map.len() == 1 {
                if let Some(oid_str) = map.get("$oid").and_then(|v| v.as_str()) {
                    if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(oid_str) {
                        return Bson::ObjectId(oid);
                    }
                }
                if let Some(date_v) = map.get("$date") {
                    // {"$date": {"$numberLong": "ms"}}
                    if let Some(ms) = date_v.as_object()
                        .and_then(|d| d.get("$numberLong"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<i64>().ok())
                    {
                        return Bson::DateTime(mongodb::bson::DateTime::from_millis(ms));
                    }
                    // {"$date": "iso_string"}
                    if let Some(iso) = date_v.as_str() {
                        if let Some(ms) = iso_str_to_millis(iso) {
                            return Bson::DateTime(mongodb::bson::DateTime::from_millis(ms));
                        }
                        return Bson::String(iso.to_string());
                    }
                }
            }
            // Regular nested object — recurse
            let mut doc = Document::new();
            for (k, v) in map {
                doc.insert(k, ejson_value_to_bson(v));
            }
            Bson::Document(doc)
        }
    }
}

fn parse_doc(s: &str) -> Result<Document, String> {
    if s.trim().is_empty() {
        return Ok(doc! {});
    }
    let json = js_to_json(s);
    let val: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("JSON parse error: {} in: {}", e, json))?;
    match ejson_value_to_bson(val) {
        mongodb::bson::Bson::Document(doc) => Ok(doc),
        other => Err(format!("Expected a BSON document, got: {:?}", other)),
    }
}

fn parse_two_docs(args: &str) -> Result<(Document, Option<Document>), String> {
    if args.trim().is_empty() {
        return Ok((doc! {}, None));
    }
    // Split at top-level comma between two objects
    let first_end = find_first_doc_end(args)?;
    let first = parse_doc(args[..first_end].trim())?;
    let rest = args[first_end..].trim_start_matches(',').trim();
    let second = if rest.is_empty() {
        None
    } else {
        Some(parse_doc(rest)?)
    };
    Ok((first, second))
}

fn parse_pipeline(args: &str) -> Result<Vec<Document>, String> {
    let json = js_to_json(args);
    let val: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("Pipeline parse error: {}", e))?;
    match val {
        serde_json::Value::Array(arr) => arr
            .into_iter()
            .map(|v| match ejson_value_to_bson(v) {
                mongodb::bson::Bson::Document(doc) => Ok(doc),
                other => Err(format!("Pipeline stage must be a document, got: {:?}", other)),
            })
            .collect(),
        _ => Err("Pipeline must be an array".to_string()),
    }
}

/// Find the character index where the first top-level JSON object ends.
fn find_first_doc_end(s: &str) -> Result<usize, String> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape { escape = false; continue; }
        if c == '\\' && in_str { escape = true; continue; }
        if c == '"' { in_str = !in_str; continue; }
        if in_str { continue; }
        if c == '{' || c == '[' { depth += 1; }
        if c == '}' || c == ']' {
            depth -= 1;
            if depth == 0 { return Ok(i + 1); }
        }
    }
    Ok(s.len())
}

/// Lightweight JS→JSON conversion for MongoDB shell syntax.
/// Handles: unquoted keys, single/double-quoted strings, BSON wrappers → EJSON,
/// new Date(...), regex literals, NumberInt/NumberLong, trailing commas, // comments.
fn js_to_json(s: &str) -> String {
    let s = remove_js_comments(s);
    let mut out = String::with_capacity(s.len() * 2);
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0usize; // char index
    let mut b = 0usize; // byte index (for starts_with checks on &str)

    while i < len {
        let c = chars[i];

        // ── Double-quoted strings: pass through verbatim (prevents false wrapper matches) ──
        if c == '"' {
            out.push('"'); b += 1; i += 1;
            while i < len {
                let ch = chars[i];
                out.push(ch); b += ch.len_utf8(); i += 1;
                if ch == '\\' && i < len {
                    // escaped char — emit and advance
                    out.push(chars[i]); b += chars[i].len_utf8(); i += 1;
                } else if ch == '"' {
                    break;
                }
            }
            continue;
        }

        // ── BSON wrappers → Extended JSON (so bson driver reconstructs proper types) ──
        // None  = emit inner as plain number
        // Some("") = emit inner as plain string (kept for fallback)
        // Some("key") = emit {"key":"inner"}
        let wrappers: &[(&str, Option<&str>)] = &[
            ("ObjectId(",      Some("$oid")),
            ("ISODate(",       Some("$date")),
            ("Date(",          Some("$date")),
            ("NumberDecimal(", Some("$numberDecimal")),
            ("NumberLong(",    None),
            ("NumberInt(",     None),
            ("Timestamp(",     Some("$timestamp")),
            ("UUID(",          Some("$uuid")),
            ("BinData(",       Some("$binary")),
        ];
        let mut matched = false;
        for (prefix, ejson_key) in wrappers {
            if s[b..].starts_with(prefix) {
                let prefix_chars = prefix.chars().count();
                i += prefix_chars; b += prefix.len();
                // skip optional opening quote
                if i < len && (chars[i] == '"' || chars[i] == '\'') { b += 1; i += 1; }
                // collect inner value (up to closing quote or ')')
                let start = i;
                while i < len && chars[i] != '"' && chars[i] != '\'' && chars[i] != ')' {
                    b += chars[i].len_utf8(); i += 1;
                }
                let inner: String = chars[start..i].iter().collect();
                // skip optional closing quote and ')'
                if i < len && (chars[i] == '"' || chars[i] == '\'') { b += 1; i += 1; }
                if i < len && chars[i] == ')' { b += 1; i += 1; }
                // emit
                match ejson_key {
                    None => out.push_str(&inner), // plain number: NumberInt(42) → 42
                    Some(key) => {
                        out.push('{');
                        out.push('"'); out.push_str(key); out.push('"');
                        out.push(':');
                        out.push('"'); out.push_str(&inner); out.push('"');
                        out.push('}');
                    }
                }
                matched = true;
                break;
            }
        }
        if matched { continue; }
        if i >= len { break; }

        // ── Regex literals: /pattern/flags → {"$regex":"pat","$options":"flags"} ──
        if c == '/' {
            // heuristic: not division if last meaningful output char is not ) ] " digit
            let last = out.trim_end_matches(|ch: char| ch.is_ascii_whitespace()).chars().last();
            let is_regex = !matches!(last, Some('0'..='9') | Some(')') | Some(']') | Some('"'));
            if is_regex {
                b += 1; i += 1;
                let mut pattern = String::new();
                while i < len && chars[i] != '/' {
                    if chars[i] == '\\' && i + 1 < len {
                        pattern.push('\\'); pattern.push(chars[i+1]);
                        b += chars[i].len_utf8() + chars[i+1].len_utf8(); i += 2;
                    } else {
                        b += chars[i].len_utf8(); pattern.push(chars[i]); i += 1;
                    }
                }
                if i < len && chars[i] == '/' { b += 1; i += 1; } // closing /
                let mut flags = String::new();
                while i < len && chars[i].is_alphabetic() {
                    flags.push(chars[i]); b += 1; i += 1;
                }
                out.push_str("{\"$regex\":\"");
                out.push_str(&pattern);
                out.push('"');
                if !flags.is_empty() {
                    out.push_str(",\"$options\":\"");
                    out.push_str(&flags);
                    out.push('"');
                }
                out.push('}');
                continue;
            }
        }

        // ── Single-quoted strings → double-quoted ──────────────────────────────
        if c == '\'' {
            out.push('"'); b += 1; i += 1;
            while i < len {
                let ch = chars[i];
                if ch == '\\' && i + 1 < len {
                    out.push('\\'); out.push(chars[i+1]);
                    b += ch.len_utf8() + chars[i+1].len_utf8(); i += 2; continue;
                }
                if ch == '"' { out.push('\\'); out.push('"'); b += 1; i += 1; continue; }
                if ch == '\'' { break; }
                out.push(ch); b += ch.len_utf8(); i += 1;
            }
            out.push('"');
            if i < len { b += 1; i += 1; } // skip closing '
            continue;
        }

        // ── Unquoted identifiers: quote as key if followed by ':', skip 'new' keyword ──
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
                b += chars[i].len_utf8(); i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let mut j = i;
            while j < len && (chars[j] == ' ' || chars[j] == '\t') { j += 1; }
            if word == "new" {
                // skip 'new' and following whitespace; next iteration handles Date(...) etc.
                while i < len && (chars[i] == ' ' || chars[i] == '\t') { b += 1; i += 1; }
            } else if j < len && chars[j] == ':' && !matches!(word.as_str(), "true" | "false" | "null") {
                out.push('"'); out.push_str(&word); out.push('"');
            } else {
                out.push_str(&word);
            }
            continue;
        }

        // ── Trailing commas before } or ] ──────────────────────────────────────
        if c == ',' {
            let mut j = i + 1;
            while j < len && matches!(chars[j], ' ' | '\n' | '\r' | '\t') { j += 1; }
            if j < len && matches!(chars[j], '}' | ']') {
                b += 1; i += 1; continue;
            }
        }

        out.push(c); b += c.len_utf8(); i += 1;
    }
    out
}

fn remove_js_comments(s: &str) -> String {
    let mut out = String::new();
    let mut in_str = false;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        // Block comment: /* ... */
        if !in_str && i + 1 < len && chars[i] == '/' && chars[i+1] == '*' {
            i += 2; // skip /*
            while i + 1 < len && !(chars[i] == '*' && chars[i+1] == '/') { i += 1; }
            if i + 1 < len { i += 2; } // skip */
            out.push(' '); // preserve spacing
            continue;
        }
        // Line comment: //
        if !in_str && i + 1 < len && chars[i] == '/' && chars[i+1] == '/' {
            while i < len && chars[i] != '\n' { i += 1; }
            continue;
        }
        if chars[i] == '"' { in_str = !in_str; }
        out.push(chars[i]);
        i += 1;
    }
    out
}



async fn collect_cursor(
    cursor: &mut mongodb::Cursor<Document>,
) -> Result<Vec<serde_json::Value>, String> {
    use futures_util::TryStreamExt;
    let docs: Vec<Document> = cursor.try_collect().await.map_err(|e| e.to_string())?;
    Ok(docs.into_iter().map(doc_to_json).collect())
}

fn doc_to_json(doc: Document) -> serde_json::Value {
    let val = serde_json::to_value(doc).unwrap_or(serde_json::Value::Null);
    normalize_ejson(val)
}

/// Convert BSON Extended JSON types to readable strings:
///   {"$oid": "hex"}            → `ObjectId("hex")`
///   {"$date": {$numberLong}}   → ISO 8601 string
///   {"$binary": {base64, ...}} → `Binary("base64")`
fn normalize_ejson(val: serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            if map.len() == 1 {
                if let Some(oid) = map.get("$oid").and_then(|v| v.as_str()) {
                    return serde_json::Value::String(format!("ObjectId(\"{}\")", oid));
                }
                if let Some(date_val) = map.get("$date") {
                    // {"$date": {"$numberLong": "1234567890000"}}
                    if let Some(ms) = date_val
                        .as_object()
                        .and_then(|d| d.get("$numberLong"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<i64>().ok())
                    {
                        return serde_json::Value::String(format!("ISODate(\"{}\")", millis_to_iso(ms)));
                    }
                    // {"$date": "iso_string"}
                    if let Some(s) = date_val.as_str() {
                        return serde_json::Value::String(format!("ISODate(\"{}\")", s));
                    }
                }
                if let Some(bin) = map.get("$binary") {
                    if let Some(b64) = bin
                        .as_object()
                        .and_then(|b| b.get("base64"))
                        .and_then(|v| v.as_str())
                    {
                        return serde_json::Value::String(format!("Binary(\"{}\")", b64));
                    }
                }
            }
            serde_json::Value::Object(
                map.into_iter().map(|(k, v)| (k, normalize_ejson(v))).collect(),
            )
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(normalize_ejson).collect())
        }
        other => other,
    }
}

/// Convert milliseconds since Unix epoch to an ISO 8601 string.
/// Uses Howard Hinnant's civil_from_days algorithm (no external deps).
fn millis_to_iso(ms: i64) -> String {
    let ts_secs = ms.div_euclid(1000);
    let ms_part = ms.rem_euclid(1000) as u32;
    let day_secs = ts_secs.rem_euclid(86400) as u32;
    let days: i64 = ts_secs.div_euclid(86400);

    let hh = day_secs / 3600;
    let mm = (day_secs % 3600) / 60;
    let ss = day_secs % 60;

    let z = days + 719468;
    let era: i64 = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    if ms_part == 0 {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hh, mm, ss)
    } else {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", y, m, d, hh, mm, ss, ms_part)
    }
}

// ─── get first doc field paths for autocomplete ──────────────────────────────

#[tauri::command]
pub async fn get_field_paths(
    conn: ConnectionConfig,
    db_name: String,
    collection: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<Vec<String>, String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    let db = client.database(&db_name);
    let col = db.collection::<Document>(&collection);
    let doc = col.find_one(doc! {}).await.map_err(|e| e.to_string())?;
    Ok(doc.map(|d| extract_paths(&d, "")).unwrap_or_default())
}

fn extract_paths(doc: &Document, prefix: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for (key, val) in doc.iter() {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", prefix, key)
        };
        paths.push(full.clone());
        if let mongodb::bson::Bson::Document(nested) = val {
            paths.extend(extract_paths(nested, &full));
        }
    }
    paths
}

// ─── Collection stats ─────────────────────────────────────────────────────────

fn bson_to_i64(doc: &Document, key: &str) -> i64 {
    use mongodb::bson::Bson;
    match doc.get(key) {
        Some(Bson::Int32(v)) => *v as i64,
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Double(v)) => *v as i64,
        _ => 0,
    }
}

#[tauri::command]
pub async fn get_collection_stats(
    conn: ConnectionConfig,
    db_name: String,
    collection: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<CollectionStats, String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    let db = client.database(&db_name);
    let result = db
        .run_command(doc! { "collStats": &collection, "scale": 1 })
        .await
        .map_err(|e| e.to_string())?;

    let count = bson_to_i64(&result, "count");
    let storage_size = bson_to_i64(&result, "storageSize");
    let avg_obj_size = if count > 0 { bson_to_i64(&result, "avgObjSize") } else { 0 };
    let total_index_size = bson_to_i64(&result, "totalIndexSize");
    let index_count = bson_to_i64(&result, "nindexes");

    Ok(CollectionStats {
        ns: format!("{}.{}", db_name, collection),
        count,
        storage_size,
        avg_obj_size,
        total_index_size,
        index_count,
    })
}

// ─── Schema inference ─────────────────────────────────────────────────────────

fn bson_type_name(val: &mongodb::bson::Bson) -> &'static str {
    use mongodb::bson::Bson;
    match val {
        Bson::Double(_)             => "double",
        Bson::String(_)             => "string",
        Bson::Document(_)           => "object",
        Bson::Array(_)              => "array",
        Bson::Boolean(_)            => "bool",
        Bson::Null                  => "null",
        Bson::Int32(_)              => "int32",
        Bson::Int64(_)              => "int64",
        Bson::ObjectId(_)           => "objectId",
        Bson::DateTime(_)           => "date",
        Bson::RegularExpression(_)  => "regex",
        Bson::Binary(_)             => "binary",
        Bson::Decimal128(_)         => "decimal128",
        Bson::Timestamp(_)          => "timestamp",
        _                           => "unknown",
    }
}

fn collect_field_types(doc: &Document, prefix: &str, acc: &mut HashMap<String, HashSet<String>>) {
    for (key, val) in doc.iter() {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", prefix, key)
        };
        acc.entry(path.clone()).or_default().insert(bson_type_name(val).to_string());
        if let mongodb::bson::Bson::Document(nested) = val {
            collect_field_types(nested, &path, acc);
        }
    }
}

#[tauri::command]
pub async fn infer_schema(
    conn: ConnectionConfig,
    db_name: String,
    collection: String,
    sample_size: u32,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<Vec<SchemaField>, String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    let db = client.database(&db_name);
    let col = db.collection::<Document>(&collection);

    use mongodb::options::FindOptions;
    use futures_util::TryStreamExt;
    let opts = FindOptions::builder().limit(sample_size as i64).build();
    let cursor = col.find(doc! {}).with_options(opts).await.map_err(|e| e.to_string())?;
    let docs: Vec<Document> = cursor.try_collect().await.map_err(|e| e.to_string())?;

    let total = docs.len();
    if total == 0 {
        return Ok(vec![]);
    }

    let mut field_types: HashMap<String, HashSet<String>> = HashMap::new();
    let mut field_counts: HashMap<String, usize> = HashMap::new();

    for doc in &docs {
        let mut per_doc: HashMap<String, HashSet<String>> = HashMap::new();
        collect_field_types(doc, "", &mut per_doc);
        for (path, types) in per_doc {
            *field_counts.entry(path.clone()).or_insert(0) += 1;
            field_types.entry(path).or_default().extend(types);
        }
    }

    let mut fields: Vec<SchemaField> = field_types
        .into_iter()
        .map(|(path, types_set)| {
            let count = *field_counts.get(&path).unwrap_or(&0);
            let presence = count as f64 / total as f64;
            let mut types: Vec<String> = types_set.into_iter().collect();
            types.sort();
            SchemaField { path, types, count, presence }
        })
        .collect();
    fields.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(fields)
}

// ─── List indexes ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_indexes_cmd(
    conn: ConnectionConfig,
    db_name: String,
    collection: String,
    pool: State<'_, MongoPool>,
    tunnels: State<'_, TunnelPool>,
) -> Result<Vec<IndexInfo>, String> {
    let client = get_client(&pool, &conn, &tunnels).await?;
    let db = client.database(&db_name);
    let result = db
        .run_command(doc! { "listIndexes": &collection })
        .await
        .map_err(|e| e.to_string())?;

    let batch = result
        .get_document("cursor")
        .ok()
        .and_then(|c| c.get_array("firstBatch").ok())
        .map(|a| a.to_vec())
        .unwrap_or_default();

    let indexes = batch
        .into_iter()
        .filter_map(|bson| {
            let doc = match bson {
                mongodb::bson::Bson::Document(d) => d,
                _ => return None,
            };
            let name = doc.get_str("name").unwrap_or("").to_string();
            let unique = doc.get_bool("unique").unwrap_or(false);
            let sparse = doc.get_bool("sparse").unwrap_or(false);
            let keys = doc
                .get_document("key")
                .ok()
                .and_then(|k| serde_json::to_value(k).ok())
                .unwrap_or(serde_json::Value::Null);
            Some(IndexInfo { name, keys, unique, sparse })
        })
        .collect();

    Ok(indexes)
}
