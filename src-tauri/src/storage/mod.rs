use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// ─── Keychain helpers ─────────────────────────────────────────────────────────

fn keychain_entry(conn_id: &str) -> Option<Entry> {
    Entry::new("ferango", conn_id).ok()
}

fn store_password(conn_id: &str, password: &str) {
    if let Some(entry) = keychain_entry(conn_id) {
        let _ = entry.set_password(password);
    }
}

fn fetch_password(conn_id: &str) -> Option<String> {
    keychain_entry(conn_id)?.get_password().ok()
}

fn delete_password(conn_id: &str) {
    if let Some(entry) = keychain_entry(conn_id) {
        let _ = entry.delete_credential();
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptFile {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub content: Option<String>,
    pub modified_at: u64,
}

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub conn_id: String,
    pub db: String,
    pub query: String,
    pub elapsed_ms: u64,
}

// ─── Settings ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptContext {
    pub conn_id: String,
    pub db: String,
    pub collection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenTab {
    pub script_path: String,
    #[serde(default)]
    pub conn_id: String,
    #[serde(default)]
    pub db_name: String,
    #[serde(default)]
    pub collection_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default)]
    pub last_dbs: std::collections::HashMap<String, String>,
    #[serde(default = "default_ai_enabled")]
    pub ai_enabled: bool,
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_endpoint")]
    pub ai_endpoint: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    #[serde(default = "default_result_view")]
    pub result_view: String,
    #[serde(default)]
    pub script_contexts: std::collections::HashMap<String, ScriptContext>,
    #[serde(default)]
    pub open_tabs: Vec<OpenTab>,
    #[serde(default)]
    pub active_tab_index: i32,
}

fn default_theme() -> String { "dark".to_string() }
fn default_font_size() -> u32 { 13 }
fn default_ai_enabled() -> bool { true }
fn default_ai_provider() -> String { "ollama".to_string() }
fn default_ai_endpoint() -> String { "http://localhost:11434".to_string() }
fn default_ai_model() -> String { "codellama:7b".to_string() }
fn default_result_view() -> String { "table".to_string() }

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            theme: default_theme(),
            font_size: default_font_size(),
            last_dbs: Default::default(),
            ai_enabled: default_ai_enabled(),
            ai_provider: default_ai_provider(),
            ai_endpoint: default_ai_endpoint(),
            ai_model: default_ai_model(),
            result_view: default_result_view(),
            script_contexts: Default::default(),
            open_tabs: Default::default(),
            active_tab_index: -1,
        }
    }
}

// ─── Paths ───────────────────────────────────────────────────────────────────

fn base_dir() -> PathBuf {
    let home = dirs::home_dir().expect("Cannot find home directory");
    home.join(".ferango")
}

fn connections_file() -> PathBuf {
    base_dir().join("connections.json")
}

fn scripts_dir() -> PathBuf {
    base_dir().join("scripts")
}

fn history_dir() -> PathBuf {
    base_dir().join("history")
}

fn history_file(conn_id: &str) -> PathBuf {
    history_dir().join(format!("{}.jsonl", conn_id))
}

fn settings_file() -> PathBuf {
    base_dir().join("settings.json")
}

fn ensure_dir(p: &PathBuf) {
    if !p.exists() {
        fs::create_dir_all(p).expect("Failed to create directory");
    }
}

// ─── Connections ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn load_connections() -> Result<Vec<ConnectionConfig>, String> {
    let f = connections_file();
    if !f.exists() {
        return Ok(vec![]);
    }
    let raw = fs::read_to_string(&f).map_err(|e| e.to_string())?;
    let mut conns: Vec<ConnectionConfig> = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    // Inject passwords from keychain (passwords are not stored in JSON)
    for conn in &mut conns {
        if conn.password.as_deref().unwrap_or("").is_empty() {
            conn.password = fetch_password(&conn.id);
        }
    }
    Ok(conns)
}

#[tauri::command]
pub fn save_connection(mut conn: ConnectionConfig) -> Result<String, String> {
    ensure_dir(&base_dir());
    // Load raw connections (without keychain injection) to avoid re-saving fetched passwords
    let raw_conns = {
        let f = connections_file();
        if f.exists() {
            let raw = fs::read_to_string(&f).map_err(|e| e.to_string())?;
            serde_json::from_str::<Vec<ConnectionConfig>>(&raw).unwrap_or_default()
        } else {
            vec![]
        }
    };
    let mut conns = raw_conns;

    // Move password to keychain; strip from struct
    if let Some(pw) = conn.password.take() {
        if !pw.is_empty() {
            if conn.id.is_empty() {
                // ID not yet assigned; we'll store after generating
                conn.password = Some(pw); // hold temporarily
            } else {
                store_password(&conn.id, &pw);
            }
        }
    }

    if conn.id.is_empty() {
        let id = format!(
            "conn_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        conn.id = id.clone();
        // Now store password with the real ID
        if let Some(pw) = conn.password.take() {
            store_password(&id, &pw);
        }
        conns.push(conn);
        save_all_connections(&conns)?;
        Ok(id)
    } else {
        let id = conn.id.clone();
        if let Some(existing) = conns.iter_mut().find(|c| c.id == id) {
            *existing = conn;
        } else {
            conns.push(conn);
        }
        save_all_connections(&conns)?;
        Ok(id)
    }
}

#[tauri::command]
pub fn delete_connection(id: String) -> Result<(), String> {
    delete_password(&id);
    let f = connections_file();
    if !f.exists() { return Ok(()); }
    let raw = fs::read_to_string(&f).map_err(|e| e.to_string())?;
    let mut conns: Vec<ConnectionConfig> = serde_json::from_str(&raw).unwrap_or_default();
    conns.retain(|c| c.id != id);
    save_all_connections(&conns)
}

fn save_all_connections(conns: &[ConnectionConfig]) -> Result<(), String> {
    ensure_dir(&base_dir());
    let json = serde_json::to_string_pretty(conns).map_err(|e| e.to_string())?;
    fs::write(connections_file(), json).map_err(|e| e.to_string())
}

// ─── Import connections ───────────────────────────────────────────────────────

/// Parse a Compass connections export file and return importable ConnectionConfigs.
/// Supports Compass ≥ 1.28 format (array of objects with connectionOptions.connectionString)
/// and legacy format (array of objects with hostname/port/auth fields).
#[tauri::command]
pub fn parse_compass_file(path: String) -> Result<Vec<ConnectionConfig>, String> {
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let val: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let arr = val.as_array().ok_or("Expected a JSON array")?;
    let mut results = Vec::new();

    for item in arr {
        let name = item
            .pointer("/favorite/name")
            .or_else(|| item.pointer("/name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Imported")
            .to_string();

        // Compass ≥ 1.28: connectionOptions.connectionString
        if let Some(uri) = item.pointer("/connectionOptions/connectionString").and_then(|v| v.as_str()) {
            results.push(ConnectionConfig {
                id: String::new(),
                name,
                uri: Some(uri.to_string()),
                host: None, port: None, user: None, password: None, options: None,
                group: Some("Imported".to_string()),
                ssh_host: None, ssh_port: None, ssh_user: None, ssh_key_path: None,
            });
            continue;
        }

        // Legacy format: hostname, port, auth.username/password
        if let Some(host) = item.get("hostname").and_then(|v| v.as_str()) {
            let port = item.get("port").and_then(|v| v.as_u64()).unwrap_or(27017).to_string();
            let user = item.pointer("/auth/username").and_then(|v| v.as_str()).map(String::from);
            let password = item.pointer("/auth/password").and_then(|v| v.as_str()).map(String::from);
            results.push(ConnectionConfig {
                id: String::new(),
                name,
                uri: None,
                host: Some(host.to_string()),
                port: Some(port),
                user,
                password,
                options: None,
                group: Some("Imported".to_string()),
                ssh_host: None, ssh_port: None, ssh_user: None, ssh_key_path: None,
            });
        }
    }

    Ok(results)
}

// ─── Scripts ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_scripts() -> Result<Vec<ScriptFile>, String> {
    let base = scripts_dir();
    ensure_dir(&base);
    let mut result = Vec::new();

    let folders = fs::read_dir(&base).map_err(|e| e.to_string())?;
    for folder_entry in folders.flatten() {
        if !folder_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let folder_name = folder_entry.file_name().to_string_lossy().to_string();
        let files = fs::read_dir(folder_entry.path()).map_err(|e| e.to_string())?;
        for file_entry in files.flatten() {
            let fname = file_entry.file_name().to_string_lossy().to_string();
            if fname.ends_with(".js") {
                let name = fname.trim_end_matches(".js").to_string();
                let modified_at = file_entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                result.push(ScriptFile {
                    name,
                    path: file_entry.path().to_string_lossy().to_string(),
                    folder: folder_name.clone(),
                    content: None,
                    modified_at,
                });
            }
        }
    }

    result.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(result)
}

#[tauri::command]
pub fn create_script(folder: String, name: Option<String>, content: String) -> Result<ScriptFile, String> {
    let folder_dir = scripts_dir().join(&folder);
    ensure_dir(&folder_dir);

    let script_name = match name {
        Some(n) if !n.is_empty() => n,
        _ => {
            // Auto-generate: Script_001.js
            let existing: Vec<_> = fs::read_dir(&folder_dir)
                .map(|d| d.flatten().collect())
                .unwrap_or_default();
            let mut n = existing.len() + 1;
            loop {
                let candidate = format!("Script_{:03}", n);
                if !folder_dir.join(format!("{}.js", candidate)).exists() {
                    break candidate;
                }
                n += 1;
            }
        }
    };

    let fname = if script_name.ends_with(".js") {
        script_name.clone()
    } else {
        format!("{}.js", script_name)
    };

    let path = folder_dir.join(&fname);
    fs::write(&path, &content).map_err(|e| e.to_string())?;

    Ok(ScriptFile {
        name: fname.trim_end_matches(".js").to_string(),
        path: path.to_string_lossy().to_string(),
        folder,
        content: Some(content),
        modified_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    })
}

#[tauri::command]
pub fn read_script(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_script(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_script(path: String) -> Result<(), String> {
    fs::remove_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_script(path: String, new_name: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let dir = p.parent().ok_or("Invalid path")?;
    let new_fname = if new_name.ends_with(".js") {
        new_name
    } else {
        format!("{}.js", new_name)
    };
    let new_path = dir.join(&new_fname);
    fs::rename(&p, &new_path).map_err(|e| e.to_string())?;
    Ok(new_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_scripts_dir() -> String {
    scripts_dir().to_string_lossy().to_string()
}

// ─── History ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn log_query(conn_id: String, db: String, query: String, elapsed_ms: u64) -> Result<(), String> {
    ensure_dir(&history_dir());
    let hist_path = history_file(&conn_id);
    let entry = HistoryEntry {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        conn_id,
        db,
        query,
        elapsed_ms,
    };
    let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(hist_path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{}", line).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_history(conn_id: String, limit: usize) -> Result<Vec<HistoryEntry>, String> {
    let path = history_file(&conn_id);
    if !path.exists() { return Ok(vec![]); }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut entries: Vec<HistoryEntry> = raw.lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    entries.truncate(limit);
    Ok(entries)
}

#[tauri::command]
pub fn clear_history(conn_id: String) -> Result<(), String> {
    let path = history_file(&conn_id);
    if path.exists() { fs::remove_file(&path).map_err(|e| e.to_string())?; }
    Ok(())
}

// ─── Export ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn export_scripts_zip() -> Result<String, String> {
    let base = scripts_dir();
    if !base.exists() { return Err("No scripts directory found".into()); }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let output = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!("ferango-scripts-{}.zip", ts));

    let file = fs::File::create(&output).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for folder_entry in fs::read_dir(&base).map_err(|e| e.to_string())?.flatten() {
        if !folder_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
        let folder_name = folder_entry.file_name().to_string_lossy().to_string();
        for file_entry in fs::read_dir(folder_entry.path()).map_err(|e| e.to_string())?.flatten() {
            let fname = file_entry.file_name().to_string_lossy().to_string();
            if !fname.ends_with(".js") { continue; }
            let zip_path = format!("{}/{}", folder_name, fname);
            zip.start_file(zip_path, options).map_err(|e| e.to_string())?;
            let content = fs::read(file_entry.path()).map_err(|e| e.to_string())?;
            zip.write_all(&content).map_err(|e| e.to_string())?;
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(output.to_string_lossy().to_string())
}

// ─── Settings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn load_settings() -> AppSettings {
    let path = settings_file();
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AppSettings::default()
    }
}

#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    let path = settings_file();
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let data = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())
}
