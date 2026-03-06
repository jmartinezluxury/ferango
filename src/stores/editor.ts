import { defineStore } from 'pinia'
import { ref, shallowRef, computed } from 'vue'
import type { ScriptFile, QueryResult, ConnectionConfig } from '../lib/tauri'
import { executeQuery } from '../lib/tauri'

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
}

export const useEditorStore = defineStore('editor', () => {
  const tabs = ref<Tab[]>([])
  const activeTabIndex = ref(-1)
  const allScripts = ref<ScriptFile[]>([])
  const scriptsDir = ref('')

  const _results = shallowRef<StatementResult[]>([])
  const activeResultIdx = ref(0)
  const result = computed<QueryResult | null>(() => _results.value[activeResultIdx.value]?.result ?? null)
  const results = computed(() => _results.value)
  const isExecuting = ref(false)
  const queryLimit = ref(50)

  // Set this to trigger an external "insert + execute" from another component
  const pendingExec = ref<string | null>(null)

  // Pagination
  const currentPage = ref(0)
  const _rawStmt = ref('')
  const canPaginate = computed(() => !!_rawStmt.value)

  async function init() {
    allScripts.value = await listScripts()
    scriptsDir.value = await getScriptsDir()
  }

  function activeTab(): Tab | null {
    return tabs.value[activeTabIndex.value] ?? null
  }

  async function openScript(script: ScriptFile) {
    const existing = tabs.value.findIndex(t => t.script.path === script.path)
    if (existing >= 0) {
      activeTabIndex.value = existing
      return
    }
    const content = await readScript(script.path)
    tabs.value.push({ script, content, modified: false })
    activeTabIndex.value = tabs.value.length - 1
  }

  async function newScript(folder: string, collection?: string) {
    const content = collection
      ? `db.getCollection("${collection}").find({});`
      : '// New query\n'
    const script = await createScript(folder, undefined, content)
    tabs.value.push({ script, content, modified: false })
    activeTabIndex.value = tabs.value.length - 1
    await refreshScripts()
  }

  function setContent(content: string) {
    const tab = tabs.value[activeTabIndex.value]
    if (!tab) return
    tab.content = content
    tab.modified = true
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

  async function closeTab(index: number) {
    const tab = tabs.value[index]
    if (tab?.modified) await saveScript(tab.script.path, tab.content)
    tabs.value.splice(index, 1)
    if (activeTabIndex.value >= tabs.value.length) {
      activeTabIndex.value = tabs.value.length - 1
    }
  }

  async function switchTab(index: number) {
    await autoSave()
    activeTabIndex.value = index
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

  function setResult(r: QueryResult | null) {
    _results.value = r ? [{ label: '#1', result: r }] : []
    activeResultIdx.value = 0
  }

  function setResults(items: StatementResult[], rawStmt = '') {
    _results.value = items
    activeResultIdx.value = 0
    currentPage.value = 0
    _rawStmt.value = rawStmt
  }

  async function paginate(conn: ConnectionConfig, db: string, page: number) {
    if (!_rawStmt.value) return
    currentPage.value = page
    const limit = queryLimit.value
    const stmt = `${_rawStmt.value}.skip(${page * limit}).limit(${limit})`
    isExecuting.value = true
    try {
      const res = await executeQuery(conn, db, stmt)
      _results.value = [{ label: '#1', result: res }]
      activeResultIdx.value = 0
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
    init,
    activeTab,
    openScript,
    newScript,
    setContent,
    saveActive,
    autoSave,
    closeTab,
    switchTab,
    renameActive,
    removeScript,
    refreshScripts,
    setResult,
    setResults,
    currentPage,
    canPaginate,
    paginate,
  }
})
