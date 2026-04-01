mod ai;
mod mongo;
mod ssh;
mod storage;

use mongo::MongoPool;
use ssh::TunnelPool;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(MongoPool::new())
        .manage(TunnelPool::new())
        .invoke_handler(tauri::generate_handler![
            // MongoDB commands
            mongo::test_connection,
            mongo::list_databases,
            mongo::list_collections,
            mongo::execute_query,
            mongo::disconnect,
            mongo::check_connection,
            mongo::create_collection,
            mongo::drop_collection_cmd,
            mongo::drop_database_cmd,
            mongo::get_field_paths,
            mongo::get_collection_stats,
            mongo::infer_schema,
            mongo::list_indexes_cmd,
            // Storage commands
            storage::load_connections,
            storage::save_connection,
            storage::delete_connection,
            storage::list_scripts,
            storage::create_script,
            storage::read_script,
            storage::save_script,
            storage::delete_script,
            storage::rename_script,
            storage::get_scripts_dir,
            storage::log_query,
            storage::list_history,
            storage::clear_history,
            storage::export_scripts_zip,
            storage::parse_compass_file,
            storage::load_settings,
            storage::save_settings,
            ssh::open_tunnel,
            ssh::close_tunnel,
            // AI commands
            ai::ai_complete,
            ai::ai_check_health,
            ai::save_ai_api_key,
            ai::get_ai_api_key_exists,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ferango");
}
