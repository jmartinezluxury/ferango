use std::collections::HashMap;
use std::net::TcpListener;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::State;

// ─── Tunnel pool (shared state) ───────────────────────────────────────────────

pub struct TunnelPool(pub Arc<Mutex<HashMap<String, Child>>>);

impl TunnelPool {
    pub fn new() -> Self {
        TunnelPool(Arc::new(Mutex::new(HashMap::new())))
    }
}

// ─── Pick a free local port ───────────────────────────────────────────────────

fn free_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .map(|l| l.local_addr().unwrap().port())
        .map_err(|e| format!("Cannot find free port: {}", e))
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// Internal (non-tauri) version: open or reuse a tunnel, return local port.
#[allow(clippy::too_many_arguments)]
pub fn open_tunnel_internal(
    conn_id: &str,
    ssh_host: &str,
    ssh_port: &str,
    ssh_user: &str,
    ssh_key_path: &str,
    remote_host: &str,
    remote_port: &str,
    tunnels: &TunnelPool,
) -> Result<u16, String> {
    // If a tunnel is already running for this conn, reuse it.
    // We can't easily query the local port from the existing Child, so we
    // kill and re-open. Clients are cached in MongoPool so reconnection is rare.
    {
        let mut map = tunnels.0.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = map.remove(conn_id) {
            let _ = child.kill();
        }
    }

    let local_port = free_port()?;
    let forward = format!("{}:{}:{}", local_port, remote_host, remote_port);
    let ssh_p = if ssh_port.is_empty() { "22" } else { ssh_port };
    let target = if ssh_user.is_empty() {
        ssh_host.to_string()
    } else {
        format!("{}@{}", ssh_user, ssh_host)
    };

    let mut cmd = Command::new("ssh");
    cmd.args([
        "-N",
        "-o", "StrictHostKeyChecking=no",
        "-o", "ExitOnForwardFailure=yes",
        "-o", "BatchMode=yes",
        "-p", ssh_p,
        "-L", &forward,
    ]);
    if !ssh_key_path.is_empty() {
        cmd.args(["-i", ssh_key_path]);
    }
    cmd.arg(&target);

    let child = cmd.spawn().map_err(|e| format!("Failed to spawn ssh: {}", e))?;
    thread::sleep(Duration::from_millis(800));

    tunnels.0.lock().map_err(|e| e.to_string())?.insert(conn_id.to_string(), child);
    Ok(local_port)
}

/// Tauri command: open an SSH tunnel and return the local port.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn open_tunnel(
    conn_id: String,
    ssh_host: String,
    ssh_port: String,
    ssh_user: String,
    ssh_key_path: String,
    remote_host: String,
    remote_port: String,
    tunnels: State<'_, TunnelPool>,
) -> Result<u16, String> {
    open_tunnel_internal(&conn_id, &ssh_host, &ssh_port, &ssh_user, &ssh_key_path, &remote_host, &remote_port, &tunnels)
}

/// Kill the SSH tunnel for a connection.
#[tauri::command]
pub fn close_tunnel(conn_id: String, tunnels: State<'_, TunnelPool>) -> Result<(), String> {
    let mut map = tunnels.0.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = map.remove(&conn_id) {
        let _ = child.kill();
    }
    Ok(())
}
