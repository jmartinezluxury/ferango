import { invoke } from '@tauri-apps/api/core'

export interface ConnectionConfig {
  id: string
  name: string
  uri?: string
  host?: string
  port?: string
  user?: string
  password?: string
  options?: string
  group?: string
  ssh_host?: string
  ssh_port?: string
  ssh_user?: string
  ssh_key_path?: string
}

export interface TestResult {
  success: boolean
  message: string
}

export interface QueryResult {
  success: boolean
  data: Record<string, unknown>[] | null
  error: string | null
  rows: number
  elapsed_ms: number
}

export interface ScriptFile {
  name: string
  path: string
  folder: string
  content?: string
  modified_at: number
}

// ─── MongoDB ──────────────────────────────────────────────────────────────────

export const testConnection = (conn: ConnectionConfig): Promise<TestResult> =>
  invoke('test_connection', { conn })

export const listDatabases = (conn: ConnectionConfig): Promise<string[]> =>
  invoke('list_databases', { conn })

export const listCollections = (conn: ConnectionConfig, dbName: string): Promise<string[]> =>
  invoke('list_collections', { conn, dbName })

export const executeQuery = (
  conn: ConnectionConfig,
  dbName: string,
  query: string
): Promise<QueryResult> => invoke('execute_query', { conn, dbName, query })

export const disconnect = (connId: string): Promise<void> =>
  invoke('disconnect', { connId })

export const checkConnection = (connId: string): Promise<boolean> =>
  invoke('check_connection', { connId })

export const createCollection = (conn: ConnectionConfig, dbName: string, collection: string): Promise<void> =>
  invoke('create_collection', { conn, dbName, collection })

export const dropCollectionCmd = (conn: ConnectionConfig, dbName: string, collection: string): Promise<void> =>
  invoke('drop_collection_cmd', { conn, dbName, collection })

export const dropDatabaseCmd = (conn: ConnectionConfig, dbName: string): Promise<void> =>
  invoke('drop_database_cmd', { conn, dbName })

export const getFieldPaths = (
  conn: ConnectionConfig,
  dbName: string,
  collection: string
): Promise<string[]> => invoke('get_field_paths', { conn, dbName, collection })

export interface CollectionStats {
  ns: string
  count: number
  storage_size: number
  avg_obj_size: number
  total_index_size: number
  index_count: number
}

export const getCollectionStats = (
  conn: ConnectionConfig,
  dbName: string,
  collection: string
): Promise<CollectionStats> => invoke('get_collection_stats', { conn, dbName, collection })

export interface SchemaField {
  path: string
  types: string[]
  count: number
  presence: number
}

export const inferSchema = (
  conn: ConnectionConfig,
  dbName: string,
  collection: string,
  sampleSize: number
): Promise<SchemaField[]> => invoke('infer_schema', { conn, dbName, collection, sampleSize })

export interface IndexInfo {
  name: string
  keys: Record<string, unknown>
  unique: boolean
  sparse: boolean
}

export const listIndexesCmd = (
  conn: ConnectionConfig,
  dbName: string,
  collection: string
): Promise<IndexInfo[]> => invoke('list_indexes_cmd', { conn, dbName, collection })

// ─── Storage ──────────────────────────────────────────────────────────────────

export const loadConnections = (): Promise<ConnectionConfig[]> =>
  invoke('load_connections')

export const saveConnection = (conn: ConnectionConfig): Promise<string> =>
  invoke('save_connection', { conn })

export const deleteConnection = (id: string): Promise<void> =>
  invoke('delete_connection', { id })

export const listScripts = (): Promise<ScriptFile[]> =>
  invoke('list_scripts')

export const createScript = (
  folder: string,
  name?: string,
  content?: string
): Promise<ScriptFile> => invoke('create_script', { folder, name, content: content ?? '' })

export const readScript = (path: string): Promise<string> =>
  invoke('read_script', { path })

export const saveScript = (path: string, content: string): Promise<void> =>
  invoke('save_script', { path, content })

export const deleteScript = (path: string): Promise<void> =>
  invoke('delete_script', { path })

export const renameScript = (path: string, newName: string): Promise<string> =>
  invoke('rename_script', { path, newName })

export const getScriptsDir = (): Promise<string> =>
  invoke('get_scripts_dir')

// ─── History ──────────────────────────────────────────────────────────────────

export interface HistoryEntry {
  timestamp: number
  conn_id: string
  db: string
  query: string
  elapsed_ms: number
}

export const logQuery = (connId: string, db: string, query: string, elapsedMs: number): Promise<void> =>
  invoke('log_query', { connId, db, query, elapsedMs })

export const listHistory = (connId: string, limit: number): Promise<HistoryEntry[]> =>
  invoke('list_history', { connId, limit })

export const clearHistory = (connId: string): Promise<void> =>
  invoke('clear_history', { connId })

export const exportScriptsZip = (): Promise<string> =>
  invoke('export_scripts_zip')

// ─── Import ───────────────────────────────────────────────────────────────────

export const parseCompassFile = (path: string): Promise<ConnectionConfig[]> =>
  invoke('parse_compass_file', { path })

// ─── SSH Tunnel ───────────────────────────────────────────────────────────────

export const openTunnel = (connId: string, sshHost: string, sshPort: string, sshUser: string, sshKeyPath: string, remoteHost: string, remotePort: string): Promise<number> =>
  invoke('open_tunnel', { connId, sshHost, sshPort, sshUser, sshKeyPath, remoteHost, remotePort })

export const closeTunnel = (connId: string): Promise<void> =>
  invoke('close_tunnel', { connId })

// ─── Settings ─────────────────────────────────────────────────────────────────

export interface ScriptContext {
  conn_id: string
  db: string
  collection: string
}

export interface OpenTab {
  script_path: string
  conn_id: string
  db_name: string
  collection_name: string
}

export interface AppSettings {
  theme: string
  font_size: number
  last_dbs: Record<string, string>
  ai_enabled: boolean
  ai_provider: string
  ai_endpoint: string
  ai_model: string
  result_view: string
  script_contexts: Record<string, ScriptContext>
  open_tabs: OpenTab[]
  active_tab_index: number
}

export const loadSettings = (): Promise<AppSettings> =>
  invoke('load_settings')

export const saveSettings = (settings: AppSettings): Promise<void> =>
  invoke('save_settings', { settings })

// ─── AI ──────────────────────────────────────────────────────────────────────

export interface AiCompletionRequest {
  prefix: string
  suffix: string
  collection?: string
  db?: string
  field_names: string[]
}

export interface AiCompletionResponse {
  text: string
}

export const aiComplete = (request: AiCompletionRequest): Promise<AiCompletionResponse> =>
  invoke('ai_complete', { request })

export const aiCheckHealth = (): Promise<boolean> =>
  invoke('ai_check_health')

export const saveAiApiKey = (provider: string, key: string): Promise<void> =>
  invoke('save_ai_api_key', { provider, key })

export const getAiApiKeyExists = (provider: string): Promise<boolean> =>
  invoke('get_ai_api_key_exists', { provider })
