import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { ConnectionConfig } from '../lib/tauri'
import {
  loadConnections,
  saveConnection,
  deleteConnection as deleteConnectionCmd,
  listDatabases,
  listCollections,
  testConnection,
  disconnect as disconnectCmd,
  checkConnection,
} from '../lib/tauri'
import { useSettingsStore } from './settings'

export interface TreeNode {
  connId: string
  databases: string[]
  expanded: boolean
  expandedDbs: Record<string, string[]> // dbName -> collections
}

export const useConnectionsStore = defineStore('connections', () => {
  const connections = ref<ConnectionConfig[]>([])
  const tree = ref<Record<string, TreeNode>>({})

  // Active context
  const activeConn = ref<ConnectionConfig | null>(null)
  const activeDb = ref<string>('')
  const activeCollection = ref<string>('')

  // Remember last used DB per connection (in-memory, for auto-restore)
  const lastDbPerConn = ref<Record<string, string>>({})

  async function init() {
    connections.value = await loadConnections()
    // Restore last used DB per connection from persisted settings
    const settingsStore = useSettingsStore()
    lastDbPerConn.value = { ...settingsStore.lastDbs }
  }

  async function addOrUpdate(conn: ConnectionConfig) {
    const id = await saveConnection(conn)
    conn.id = id
    connections.value = await loadConnections()
    return id
  }

  async function remove(id: string) {
    await deleteConnectionCmd(id)
    await disconnectCmd(id)
    connections.value = await loadConnections()
    if (activeConn.value?.id === id) {
      activeConn.value = null
      activeDb.value = ''
      activeCollection.value = ''
    }
    delete tree.value[id]
  }

  async function toggleConnection(conn: ConnectionConfig) {
    const node = tree.value[conn.id]
    if (node?.expanded) {
      tree.value[conn.id] = { ...node, expanded: false }
      return
    }
    activeConn.value = conn
    if (!node) {
      tree.value[conn.id] = { connId: conn.id, databases: [], expanded: true, expandedDbs: {} }
    } else {
      tree.value[conn.id].expanded = true
    }
    if (!tree.value[conn.id].databases.length) {
      const dbs = await listDatabases(conn)
      tree.value[conn.id].databases = dbs
    }
  }

  async function toggleDatabase(conn: ConnectionConfig, dbName: string) {
    const node = tree.value[conn.id]
    if (!node) return

    if (node.expandedDbs[dbName]) {
      const updated = { ...node.expandedDbs }
      delete updated[dbName]
      tree.value[conn.id] = { ...node, expandedDbs: updated }
      return
    }

    activeConn.value = conn
    activeDb.value = dbName
    lastDbPerConn.value[conn.id] = dbName
    useSettingsStore().saveLastDb(conn.id, dbName).catch(() => {})
    const cols = await listCollections(conn, dbName)
    tree.value[conn.id] = {
      ...node,
      expandedDbs: { ...node.expandedDbs, [dbName]: cols },
    }
  }

  function selectCollection(conn: ConnectionConfig, dbName: string, colName: string) {
    activeConn.value = conn
    activeDb.value = dbName
    activeCollection.value = colName
    lastDbPerConn.value[conn.id] = dbName
    useSettingsStore().saveLastDb(conn.id, dbName).catch(() => {})
  }

  // Activate a DB without toggling: expand if needed, always sets it active
  async function activateDatabase(conn: ConnectionConfig, dbName: string) {
    const node = tree.value[conn.id]
    if (!node) return
    activeConn.value = conn
    activeDb.value = dbName
    lastDbPerConn.value[conn.id] = dbName
    useSettingsStore().saveLastDb(conn.id, dbName).catch(() => {})
    if (!node.expandedDbs[dbName]) {
      const cols = await listCollections(conn, dbName)
      tree.value[conn.id] = { ...node, expandedDbs: { ...node.expandedDbs, [dbName]: cols } }
    }
  }

  // Expand the tree to show conn > dbName, ensuring the nodes are visible
  // Used when switching tabs to restore the visual tree state
  async function expandToContext(conn: ConnectionConfig, dbName: string) {
    // Ensure connection node exists and is expanded
    if (!tree.value[conn.id]) {
      tree.value[conn.id] = { connId: conn.id, databases: [], expanded: true, expandedDbs: {} }
      try {
        const dbs = await listDatabases(conn)
        tree.value[conn.id] = { ...tree.value[conn.id], databases: dbs }
      } catch { /* ignore */ }
    } else {
      tree.value[conn.id] = { ...tree.value[conn.id], expanded: true }
    }

    // Ensure the DB is expanded (collections list loaded and visible)
    if (!tree.value[conn.id].expandedDbs[dbName]) {
      try {
        const cols = await listCollections(conn, dbName)
        const node = tree.value[conn.id]
        tree.value[conn.id] = { ...node, expandedDbs: { ...node.expandedDbs, [dbName]: cols } }
      } catch { /* ignore */ }
    }
  }

  async function ensureConnected(connId: string): Promise<boolean> {
    const conn = connections.value.find(c => c.id === connId)
    if (!conn) return false

    // Check if existing client is alive
    const alive = await checkConnection(connId).catch(() => false)
    if (alive) return true

    // Connection dead or not in pool — reconnect by listing databases
    // (get_client in Rust will create a fresh client)
    try {
      const dbs = await listDatabases(conn)
      if (tree.value[conn.id]) {
        tree.value[conn.id] = { ...tree.value[conn.id], databases: dbs, expanded: true }
      } else {
        tree.value[conn.id] = { connId: conn.id, databases: dbs, expanded: true, expandedDbs: {} }
      }
      return true
    } catch {
      return false
    }
  }

  async function test(conn: ConnectionConfig) {
    return testConnection(conn)
  }

  async function refreshCollections(conn: ConnectionConfig, dbName: string) {
    const cols = await listCollections(conn, dbName)
    if (tree.value[conn.id]) {
      tree.value[conn.id].expandedDbs[dbName] = cols
    }
    return cols
  }

  return {
    connections,
    tree,
    activeConn,
    activeDb,
    activeCollection,
    lastDbPerConn,
    init,
    addOrUpdate,
    remove,
    toggleConnection,
    toggleDatabase,
    selectCollection,
    expandToContext,
    activateDatabase,
    ensureConnected,
    test,
    refreshCollections,
  }
})
