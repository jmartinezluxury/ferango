import { defineStore } from 'pinia'
import { ref, shallowRef, computed } from 'vue'
import type { ScriptFile, QueryResult, ConnectionConfig, OpenTab } from '../lib/tauri'
import { executeQuery } from '../lib/tauri'
import { useConnectionsStore } from './connections'
import { useSettingsStore } from './settings'

export interface StatementResult {
  label: string
  result: QueryResult
}
import {
  createScript,
  readScript,
  saveScript,
  deleteScript,
  listScripts,
  renameScript,
  getScriptsDir,
} from '../lib/tauri'

export interface Tab {
  script: ScriptFile
  content: string
  modified: boolean
  // Per-tab result state
  results: StatementResult[]
  activeResultIdx: number
  queryLimit: number
  currentPage: number
  rawStmt: string
  // Per-tab connection context (saved on execute, restored on switch)
  connId: string | null
  dbName: string
  collectionName: string
}

function makeTab(script: ScriptFile, content: string): Tab {
  return {
    script,
    content,
    modified: false,
    results: [],
    activeResultIdx: 0,
    queryLimit: 50,
    currentPage: 0,
    rawStmt: '',
    connId: null,
    dbName: '',
    collectionName: '',
  }
}

export const useEditorStore = defineStore('editor', () => {
  const tabs = shallowRef<Tab[]>([])
  const activeTabIndex = ref(-1)
  const allScripts = ref<ScriptFile[]>([])
  const scriptsDir = ref('')

  const isExecuting = ref(false)

  // Set this to trigger an external "insert + execute" from another component
  const pendingExec = ref<string | null>(null)

  function _persistTabs() {
    const settingsStore = useSettingsStore()
    const openTabsList: OpenTab[] = tabs.value.map(t => ({
      script_path: t.script.path,
      conn_id: t.connId ?? '',
      db_name: t.dbName,
      collection_name: t.collectionName,
    }))
    settingsStore.saveOpenTabs(openTabsList, activeTabIndex.value).catch(() => {})
  }

  async function init() {
    allScripts.value = await listScripts()
    scriptsDir.value = await getScriptsDir()

    // Restore previously open tabs from settings
    const settingsStore = useSettingsStore()
    const saved = settingsStore.openTabs
    if (saved.length) {
      const restored: Tab[] = []
      for (const entry of saved) {
        // Find matching script in allScripts
        const script = allScripts.value.find(s => s.path === entry.script_path)
        if (!script) continue
        try {
          const content = await readScript(script.path)
          const tab = makeTab(script, content)
          tab.connId = entry.conn_id || null
          tab.dbName = entry.db_name || ''
          tab.collectionName = entry.collection_name || ''
          restored.push(tab)
        } catch {
          // Script file may have been deleted — skip
        }
      }
      if (restored.length) {
        tabs.value = restored
        const savedIdx = settingsStore.activeTabIdx
        activeTabIndex.value = savedIdx >= 0 && savedIdx < restored.length ? savedIdx : 0
      }
    }
  }

  function activeTab(): Tab | null {
    return tabs.value[activeTabIndex.value] ?? null
  }

  // ── Per-tab computed state ────────────────────────────────────────────────

  const results = computed(() => activeTab()?.results ?? [])

  const result = computed<QueryResult | null>(() => {
    const tab = activeTab()
    if (!tab?.results.length) return null
    return tab.results[tab.activeResultIdx]?.result ?? null
  })

  const activeResultIdx = computed({
    get: () => activeTab()?.activeResultIdx ?? 0,
    set: (v: number) => {
      const t = activeTab()
      if (t) { t.activeResultIdx = v; triggerTabsUpdate() }
    },
  })

  const queryLimit = computed({
    get: () => activeTab()?.queryLimit ?? 50,
    set: (v: number) => {
      const t = activeTab()
      if (t) { t.queryLimit = v; triggerTabsUpdate() }
    },
  })

  const currentPage = computed({
    get: () => activeTab()?.currentPage ?? 0,
    set: (v: number) => {
      const t = activeTab()
      if (t) { t.currentPage = v; triggerTabsUpdate() }
    },
  })

  const canPaginate = computed(() => !!(activeTab()?.rawStmt))

  // shallowRef needs a manual trigger when we mutate nested objects
  function triggerTabsUpdate() {
    tabs.value = [...tabs.value]
  }

  // ── Script / tab management ───────────────────────────────────────────────

  async function openScript(script: ScriptFile) {
    const existing = tabs.value.findIndex(t => t.script.path === script.path)
    if (existing >= 0) {
      await switchTab(existing)
      return
    }
    const content = await readScript(script.path)
    const tab = makeTab(script, content)

    // Restore persisted connection context for this script
    const settingsStore = useSettingsStore()
    const savedCtx = settingsStore.scriptContexts[script.path]
    if (savedCtx?.conn_id && savedCtx.db) {
      tab.connId = savedCtx.conn_id
      tab.dbName = savedCtx.db
      tab.collectionName = savedCtx.collection
    }

    tabs.value = [...tabs.value, tab]
    activeTabIndex.value = tabs.value.length - 1

    // If we restored a context, expand the tree and set it active
    if (tab.connId) {
      const connStore = useConnectionsStore()
      const conn = connStore.connections.find((c: ConnectionConfig) => c.id === tab.connId)
      if (conn) {
        connStore.selectCollection(conn, tab.dbName, tab.collectionName)
        connStore.expandToContext(conn, tab.dbName)
      }
    }
    _persistTabs()
  }

  async function newScript(folder: string, collection?: string) {
    const content = collection
      ? `db.getCollection("${collection}").find({});`
      : '// New query\n'
    const script = await createScript(folder, undefined, content)
    tabs.value = [...tabs.value, makeTab(script, content)]
    activeTabIndex.value = tabs.value.length - 1
    await refreshScripts()
    _persistTabs()
  }

  function setContent(content: string) {
    const tab = tabs.value[activeTabIndex.value]
    if (!tab) return
    tab.content = content
    tab.modified = true
  }

  function setTabContent(index: number, content: string) {
    const tab = tabs.value[index]
    if (!tab) return
    tab.content = content
    tab.modified = true
    triggerTabsUpdate()
  }

  async function saveTabAt(index: number) {
    const tab = tabs.value[index]
    if (!tab) return
    await saveScript(tab.script.path, tab.content)
    tab.modified = false
    triggerTabsUpdate()
  }

  async function saveActive() {
    const tab = tabs.value[activeTabIndex.value]
    if (!tab) return
    await saveScript(tab.script.path, tab.content)
    tab.modified = false
  }

  async function autoSave() {
    const tab = tabs.value[activeTabIndex.value]
    if (tab?.modified) await saveActive()
  }

  async function saveAll() {
    for (const tab of tabs.value) {
      if (tab.modified) {
        await saveScript(tab.script.path, tab.content)
        tab.modified = false
      }
    }
  }

  async function closeTab(index: number) {
    const tab = tabs.value[index]
    if (tab?.modified) await saveScript(tab.script.path, tab.content)
    const newTabs = [...tabs.value]
    newTabs.splice(index, 1)
    tabs.value = newTabs
    if (activeTabIndex.value >= tabs.value.length) {
      activeTabIndex.value = tabs.value.length - 1
    }
    _persistTabs()
  }

  async function switchTab(index: number) {
    await autoSave()
    activeTabIndex.value = index

    // Restore connection context and expand the tree to show it
    const tab = tabs.value[index]
    if (tab?.connId && tab.dbName) {
      const connStore = useConnectionsStore()
      const conn = connStore.connections.find((c: ConnectionConfig) => c.id === tab.connId)
      if (conn) {
        // Ensure connection is alive (reconnect if needed), then restore UI context
        connStore.ensureConnected(tab.connId).catch(() => {})
        connStore.selectCollection(conn, tab.dbName, tab.collectionName)
        connStore.expandToContext(conn, tab.dbName)
      }
    }
    _persistTabs()
  }

  async function renameActive(newName: string) {
    const tab = tabs.value[activeTabIndex.value]
    if (!tab) return
    const newPath = await renameScript(tab.script.path, newName)
    tab.script = { ...tab.script, path: newPath, name: newName }
    await refreshScripts()
  }

  async function removeScript(path: string) {
    await deleteScript(path)
    const idx = tabs.value.findIndex(t => t.script.path === path)
    if (idx >= 0) await closeTab(idx)
    await refreshScripts()
  }

  async function refreshScripts() {
    allScripts.value = await listScripts()
  }

  // ── Results ───────────────────────────────────────────────────────────────

  function _saveConnContext(tab: Tab) {
    const connStore = useConnectionsStore()
    tab.connId = connStore.activeConn?.id ?? null
    tab.dbName = connStore.activeDb ?? ''
    tab.collectionName = connStore.activeCollection ?? ''
    // Persist to settings so context survives app restarts
    if (tab.connId && tab.dbName) {
      useSettingsStore().saveScriptContext(tab.script.path, tab.connId, tab.dbName, tab.collectionName)
        .catch(() => {})
    }
    _persistTabs()
  }

  function setResult(r: QueryResult | null) {
    const tab = activeTab()
    if (!tab) return
    tab.results = r ? [{ label: '#1', result: r }] : []
    tab.activeResultIdx = 0
    _saveConnContext(tab)
    triggerTabsUpdate()
  }

  function setResults(items: StatementResult[], rawStmt = '') {
    const tab = activeTab()
    if (!tab) return
    tab.results = items
    tab.activeResultIdx = 0
    tab.currentPage = 0
    tab.rawStmt = rawStmt
    _saveConnContext(tab)
    triggerTabsUpdate()
  }

  async function paginate(conn: ConnectionConfig, db: string, page: number) {
    const tab = activeTab()
    if (!tab?.rawStmt) return
    tab.currentPage = page
    const limit = tab.queryLimit
    const stmt = `${tab.rawStmt}.skip(${page * limit}).limit(${limit})`
    isExecuting.value = true
    try {
      const res = await executeQuery(conn, db, stmt)
      tab.results = [{ label: '#1', result: res }]
      tab.activeResultIdx = 0
      triggerTabsUpdate()
    } finally {
      isExecuting.value = false
    }
  }

  async function paginateTab(tabIndex: number, page: number) {
    const tab = tabs.value[tabIndex]
    if (!tab?.rawStmt) return
    const connStore = useConnectionsStore()
    const conn = tab.connId
      ? connStore.connections.find((c: ConnectionConfig) => c.id === tab.connId)
      : connStore.activeConn
    const db = tab.dbName || connStore.activeDb
    if (!conn || !db) return
    tab.currentPage = page
    const limit = tab.queryLimit
    const stmt = `${tab.rawStmt}.skip(${page * limit}).limit(${limit})`
    isExecuting.value = true
    try {
      const res = await executeQuery(conn, db, stmt)
      tab.results = [{ label: '#1', result: res }]
      tab.activeResultIdx = 0
      triggerTabsUpdate()
    } finally {
      isExecuting.value = false
    }
  }

  return {
    tabs,
    activeTabIndex,
    allScripts,
    scriptsDir,
    result,
    results,
    activeResultIdx,
    isExecuting,
    queryLimit,
    pendingExec,
    triggerTabsUpdate,
    init,
    activeTab,
    openScript,
    newScript,
    setContent,
    saveActive,
    saveAll,
    autoSave,
    closeTab,
    switchTab,
    renameActive,
    removeScript,
    refreshScripts,
    setResult,
    setResults,
    setTabContent,
    saveTabAt,
    currentPage,
    canPaginate,
    paginate,
    paginateTab,
  }
})
