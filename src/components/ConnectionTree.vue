<script setup lang="ts">
import { ref, inject, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { open as openFilePicker } from '@tauri-apps/plugin-dialog'
import { useConnectionsStore } from '../stores/connections'
import { useEditorStore } from '../stores/editor'
import type { ConnectionConfig } from '../lib/tauri'
import { Server, Database, Table2, FolderOpen, Plus, Download, Target, ChevronRight, ChevronDown, Pencil, Trash2, Search, FileText, BarChart3, ListTree, Braces, Copy } from 'lucide-vue-next'
import {
  createCollection, dropCollectionCmd, dropDatabaseCmd, parseCompassFile,
  getCollectionStats, inferSchema, listIndexesCmd, executeQuery,
} from '../lib/tauri'
import type { CollectionStats, SchemaField, IndexInfo } from '../lib/tauri'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

const connStore = useConnectionsStore()
const editorStore = useEditorStore()
const toast = inject<(msg: string, type?: 'success' | 'error' | 'info') => void>('toast')!

// ── Tree filter ──────────────────────────────────────────────────────────────
const treeFilter = ref('')
const q = computed(() => treeFilter.value.toLowerCase().trim())

function filterDbs(databases: string[]): string[] {
  if (!q.value) return databases
  return databases.filter(db => db.toLowerCase().includes(q.value))
}
function filterCols(cols: string[]): string[] {
  if (!q.value) return cols
  return cols.filter(c => c.toLowerCase().includes(q.value))
}
function connMatchesFilter(conn: ConnectionConfig): boolean {
  if (!q.value) return true
  // Connection name matches
  if (conn.name.toLowerCase().includes(q.value)) return true
  // Any database or collection matches
  const node = connStore.tree[conn.id]
  if (!node) return false
  if (node.databases.some(db => db.toLowerCase().includes(q.value))) return true
  for (const [, cols] of Object.entries(node.expandedDbs)) {
    if (cols.some(c => c.toLowerCase().includes(q.value))) return true
  }
  return false
}

// ── Connection modal ───────────────────────────────────────────────────────────
const showModal = ref(false)
const editingConn = ref<ConnectionConfig | null>(null)
const emptyForm = (): ConnectionConfig => ({
  id: '', name: '', uri: '', host: 'localhost', port: '27017',
  user: '', password: '', options: '', group: '',
  ssh_host: '', ssh_port: '22', ssh_user: '', ssh_key_path: '',
})
const form = ref<ConnectionConfig>(emptyForm())
const mode = ref<'uri' | 'params'>('params')
const showSsh = ref(false)
const testing = ref(false)
const saving = ref(false)

function openNew() {
  editingConn.value = null
  form.value = emptyForm()
  mode.value = 'params'
  showSsh.value = false
  showModal.value = true
}
function openEdit(conn: ConnectionConfig) {
  editingConn.value = conn
  form.value = { ...emptyForm(), ...conn }
  mode.value = conn.uri ? 'uri' : 'params'
  showSsh.value = !!(conn.ssh_host)
  showModal.value = true
}

// ── Connection groups ─────────────────────────────────────────────────────────
const expandedGroups = ref<Set<string>>(new Set())

function toggleGroup(g: string) {
  const s = new Set(expandedGroups.value)
  s.has(g) ? s.delete(g) : s.add(g)
  expandedGroups.value = s
}

// ── Auto-scroll tree to active node when tab context changes ─────────────────
function scrollToActiveNode() {
  nextTick(() => {
    const activeNode = document.querySelector('.tree-body .tree-col.active') as HTMLElement
      ?? document.querySelector('.tree-body .tree-db.active') as HTMLElement
      ?? document.querySelector('.tree-body .tree-conn.active') as HTMLElement
    activeNode?.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
  })
}

// Scroll on tab switch — use a short delay to let expandToContext (async) finish and tree render
watch(() => editorStore.activeTabIndex, () => {
  // Immediate attempt (works if tree is already expanded)
  scrollToActiveNode()
  // Delayed attempt (works after async expandToContext completes)
  setTimeout(() => scrollToActiveNode(), 400)
})

// ── Focus on active tab's DB context ─────────────────────────────────────────
async function focusActiveTabDb() {
  const tab = editorStore.activeTab()
  if (!tab?.connId || !tab.dbName) {
    toast('No database context in current tab', 'info')
    return
  }
  const conn = connStore.connections.find(c => c.id === tab.connId)
  if (!conn) { toast('Connection not found', 'error'); return }

  // Ensure connected, expand tree, and select
  await connStore.ensureConnected(conn.id)
  connStore.selectCollection(conn, tab.dbName, tab.collectionName)
  await connStore.expandToContext(conn, tab.dbName)

  // Scroll the tree node into view
  const el = document.querySelector(`.tree-conn[data-conn-id="${conn.id}"]`)
  el?.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
}

const groupedConnections = computed(() => {
  const groups: Record<string, ConnectionConfig[]> = {}
  const ungrouped: ConnectionConfig[] = []
  for (const c of connStore.connections) {
    if (c.group?.trim()) {
      const g = c.group.trim()
      ;(groups[g] ??= []).push(c)
    } else {
      ungrouped.push(c)
    }
  }
  return { groups, ungrouped }
})

// All unique group names across connections (for datalist)
const allGroups = computed(() =>
  [...new Set(connStore.connections.map(c => c.group?.trim()).filter(Boolean) as string[])]
)
async function testConn() {
  testing.value = true
  const c: ConnectionConfig = mode.value === 'uri'
    ? { ...form.value, host: '', port: '', user: '', password: '', options: '' }
    : { ...form.value, uri: '' }
  const result = await connStore.test(c)
  toast(result.message, result.success ? 'success' : 'error')
  testing.value = false
}
async function save() {
  if (!form.value.name.trim()) { toast('Name is required', 'error'); return }
  saving.value = true
  const base: ConnectionConfig = mode.value === 'uri'
    ? { ...form.value, host: '', port: '', user: '', password: '', options: '' }
    : { ...form.value, uri: '' }
  // Normalize optional string fields — send undefined for empty strings
  const c: ConnectionConfig = {
    ...base,
    group: base.group?.trim() || undefined,
    ssh_host: showSsh.value ? base.ssh_host?.trim() || undefined : undefined,
    ssh_port: showSsh.value ? base.ssh_port?.trim() || undefined : undefined,
    ssh_user: showSsh.value ? base.ssh_user?.trim() || undefined : undefined,
    ssh_key_path: showSsh.value ? base.ssh_key_path?.trim() || undefined : undefined,
  }
  await connStore.addOrUpdate(c)
  toast('Connection saved', 'success')
  showModal.value = false
  saving.value = false
}
async function remove(id: string, e: Event) {
  e.stopPropagation()
  if (!confirm('Delete this connection?')) return
  await connStore.remove(id)
  toast('Connection deleted', 'info')
}

// ── Tree interactions ─────────────────────────────────────────────────────────
// Track which connections are currently loading their DB list
const loadingConns = ref<Set<string>>(new Set())

async function onToggleConn(conn: ConnectionConfig) {
  // Only show spinner when expanding (not when collapsing)
  const wasExpanded = connStore.tree[conn.id]?.expanded
  if (!wasExpanded) {
    loadingConns.value = new Set([...loadingConns.value, conn.id])
  }
  try { await connStore.toggleConnection(conn) }
  catch (e) { toast(String(e), 'error') }
  finally {
    const s = new Set(loadingConns.value)
    s.delete(conn.id)
    loadingConns.value = s
  }
}
async function onToggleDb(conn: ConnectionConfig, db: string) {
  try { await connStore.toggleDatabase(conn, db) }
  catch (e) { toast(String(e), 'error') }
  // Sync to active tab so breadcrumb and execution stay consistent
  editorStore.updateActiveTabContext(conn.id, db, '')
}
function onSelectCol(conn: ConnectionConfig, db: string, col: string) {
  connStore.selectCollection(conn, db, col)
  editorStore.updateActiveTabContext(conn.id, db, col)
}

// ── Context menu ──────────────────────────────────────────────────────────────
const ctxMenu = ref<{
  show: boolean; x: number; y: number; type: 'db' | 'col'
  conn: ConnectionConfig | null; db: string; col: string
}>({ show: false, x: 0, y: 0, type: 'col', conn: null, db: '', col: '' })

const lastScriptForCtx = computed(() => {
  if (!ctxMenu.value.conn) return null
  const scripts = editorStore.allScripts.filter(s => s.folder === ctxMenu.value.conn!.name)
  return scripts.length ? scripts[scripts.length - 1] : null
})

function openCtxMenu(e: MouseEvent, type: 'db' | 'col', conn: ConnectionConfig, db: string, col: string) {
  e.preventDefault()
  e.stopPropagation()
  ctxMenu.value = { show: true, x: e.clientX, y: e.clientY, type, conn, db, col }
}
function closeCtxMenu() { ctxMenu.value.show = false }

// ── Collection-level actions ───────────────────────────────────────────────────
function ctxFindAll() {
  const { conn, db, col } = ctxMenu.value
  closeCtxMenu()
  if (!conn || !db || !col) return
  connStore.selectCollection(conn, db, col)
  editorStore.pendingExec = `db.getCollection("${col}").find({});`
}

async function ctxNewScript() {
  const { conn, col } = ctxMenu.value
  closeCtxMenu()
  if (!conn) return
  try { await editorStore.newScript(conn.name, col) }
  catch (e) { toast(String(e), 'error') }
}
async function ctxOpenLastScript() {
  const script = lastScriptForCtx.value
  closeCtxMenu()
  if (!script) return
  try { await editorStore.openScript(script) }
  catch (e) { toast(String(e), 'error') }
}

// ── Copy URI ──────────────────────────────────────────────────────────────────
async function ctxCopyUri() {
  const { conn, db } = ctxMenu.value
  closeCtxMenu()
  if (!conn) return
  const base = conn.uri?.trim() || `mongodb://${conn.host || 'localhost'}:${conn.port || '27017'}`
  const uri = db ? `${base}/${db}` : base
  await navigator.clipboard.writeText(uri)
  toast('URI copied to clipboard', 'success')
}

// ── Confirm dialog ────────────────────────────────────────────────────────────
const confirmDlg = ref<{ show: boolean; message: string; pending: (() => Promise<void>) | null }>({
  show: false, message: '', pending: null,
})
function askConfirm(message: string, onYes: () => Promise<void>) {
  confirmDlg.value = { show: true, message, pending: onYes }
}
async function doConfirm() {
  const fn = confirmDlg.value.pending
  confirmDlg.value = { show: false, message: '', pending: null }
  try { await fn?.() } catch (e) { toast(String(e), 'error') }
}
function cancelConfirm() { confirmDlg.value = { show: false, message: '', pending: null } }

// ── Drop collection ───────────────────────────────────────────────────────────
function ctxDropCollection() {
  const { conn, db, col } = ctxMenu.value
  closeCtxMenu()
  if (!conn) return
  askConfirm(`Drop collection "${col}"? This cannot be undone.`, async () => {
    await dropCollectionCmd(conn, db, col)
    await connStore.refreshCollections(conn, db)
    toast(`Collection "${col}" dropped`, 'info')
  })
}

// ── Drop database ─────────────────────────────────────────────────────────────
function ctxDropDatabase() {
  const { conn, db } = ctxMenu.value
  closeCtxMenu()
  if (!conn) return
  askConfirm(`Drop database "${db}"? This will delete ALL collections and data.`, async () => {
    await dropDatabaseCmd(conn, db)
    // Remove db from tree without a full reconnect
    const node = connStore.tree[conn.id]
    if (node) {
      const expandedDbs = { ...node.expandedDbs }
      delete expandedDbs[db]
      connStore.tree[conn.id] = {
        ...node,
        databases: node.databases.filter(d => d !== db),
        expandedDbs,
      }
    }
    toast(`Database "${db}" dropped`, 'info')
  })
}

// ── Create collection dialog ──────────────────────────────────────────────────
const newColDlg = ref<{ show: boolean; name: string; conn: ConnectionConfig | null; db: string }>({
  show: false, name: '', conn: null, db: '',
})
function ctxCreateCollection() {
  const { conn, db } = ctxMenu.value
  closeCtxMenu()
  if (!conn) return
  newColDlg.value = { show: true, name: '', conn, db }
}
async function submitNewCol() {
  const { conn, db, name } = newColDlg.value
  newColDlg.value.show = false
  if (!name.trim() || !conn) return
  try {
    await createCollection(conn, db, name.trim())
    await connStore.refreshCollections(conn, db)
    toast(`Collection "${name.trim()}" created`, 'success')
  } catch (e) { toast(String(e), 'error') }
}

// ── Import connections ─────────────────────────────────────────────────────────
const importDlg = ref<{ show: boolean; candidates: (ConnectionConfig & { selected: boolean })[] }>({
  show: false, candidates: [],
})

async function openImport() {
  const file = await openFilePicker({
    title: 'Select Compass connections.json',
    filters: [{ name: 'JSON', extensions: ['json'] }],
    multiple: false,
  })
  const path = typeof file === 'string' ? file : (file as { path?: string } | null)?.path
  if (!path) return
  try {
    const parsed = await parseCompassFile(path)
    if (!parsed.length) { toast('No importable connections found', 'info'); return }
    // Mark as selected by default; skip duplicates already saved
    const existingUris = new Set(connStore.connections.map(c => c.uri?.trim()).filter(Boolean))
    importDlg.value = {
      show: true,
      candidates: parsed.map(c => ({
        ...c,
        selected: !existingUris.has(c.uri?.trim()),
      })),
    }
  } catch (e) {
    toast(String(e), 'error')
  }
}

async function doImport() {
  const selected = importDlg.value.candidates.filter(c => c.selected)
  importDlg.value.show = false
  let count = 0
  for (const c of selected) {
    try { await connStore.addOrUpdate({ ...c }); count++ } catch { /* skip */ }
  }
  toast(`Imported ${count} connection(s)`, 'success')
}

// ── Collection stats ──────────────────────────────────────────────────────────
const statsDlg = ref<{ show: boolean; loading: boolean; stats: CollectionStats | null }>({
  show: false, loading: false, stats: null,
})
async function ctxViewStats() {
  const { conn, db, col } = ctxMenu.value
  closeCtxMenu()
  if (!conn || !db || !col) return
  statsDlg.value = { show: true, loading: true, stats: null }
  try {
    statsDlg.value.stats = await getCollectionStats(conn, db, col)
  } catch (e) {
    toast(String(e), 'error')
    statsDlg.value.show = false
  } finally {
    statsDlg.value.loading = false
  }
}
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`
}

// ── Schema inference ───────────────────────────────────────────────────────────
const schemaDlg = ref<{ show: boolean; loading: boolean; fields: SchemaField[]; col: string }>({
  show: false, loading: false, fields: [], col: '',
})
async function ctxInferSchema() {
  const { conn, db, col } = ctxMenu.value
  closeCtxMenu()
  if (!conn || !db || !col) return
  schemaDlg.value = { show: true, loading: true, fields: [], col }
  try {
    schemaDlg.value.fields = await inferSchema(conn, db, col, 100)
  } catch (e) {
    toast(String(e), 'error')
    schemaDlg.value.show = false
  } finally {
    schemaDlg.value.loading = false
  }
}

// ── Indexes ────────────────────────────────────────────────────────────────────
const indexesDlg = ref<{ show: boolean; loading: boolean; indexes: IndexInfo[]; col: string }>({
  show: false, loading: false, indexes: [], col: '',
})
async function ctxViewIndexes() {
  const { conn, db, col } = ctxMenu.value
  closeCtxMenu()
  if (!conn || !db || !col) return
  indexesDlg.value = { show: true, loading: true, indexes: [], col }
  try {
    indexesDlg.value.indexes = await listIndexesCmd(conn, db, col)
  } catch (e) {
    toast(String(e), 'error')
    indexesDlg.value.show = false
  } finally {
    indexesDlg.value.loading = false
  }
}

// ── Insert document ────────────────────────────────────────────────────────────
const insertDlg = ref<{
  show: boolean; json: string; error: string
  conn: ConnectionConfig | null; db: string; col: string
}>({ show: false, json: '{\n  \n}', error: '', conn: null, db: '', col: '' })

function ctxInsertDoc() {
  const { conn, db, col } = ctxMenu.value
  closeCtxMenu()
  if (!conn || !db || !col) return
  insertDlg.value = { show: true, json: '{\n  \n}', error: '', conn, db, col }
}
async function submitInsert() {
  const { conn, db, col, json } = insertDlg.value
  if (!conn) return
  try { JSON.parse(json) } catch (e) { insertDlg.value.error = String(e); return }
  insertDlg.value.error = ''
  try {
    const result = await executeQuery(conn, db, `db.getCollection("${col}").insertOne(${json})`)
    if (!result.success) { insertDlg.value.error = result.error ?? 'Insert failed'; return }
    insertDlg.value.show = false
    toast('Document inserted', 'success')
  } catch (e) { insertDlg.value.error = String(e) }
}

onMounted(() => document.addEventListener('click', closeCtxMenu))
onBeforeUnmount(() => document.removeEventListener('click', closeCtxMenu))
</script>

<template>
  <div class="tree-root">
    <div class="tree-header">
      <span class="tree-title">Connections</span>
      <span class="tree-header-actions">
        <button class="btn-icon" title="Focus on active tab's DB" @click="focusActiveTabDb"><Target class="h-3.5 w-3.5" /></button>
        <button class="btn-icon" title="Import from Compass" @click="openImport"><Download class="h-3.5 w-3.5" /></button>
        <button class="btn-icon" title="New connection" @click="openNew"><Plus class="h-3.5 w-3.5" /></button>
      </span>
    </div>

    <div v-if="connStore.connections.length" class="tree-search">
      <input v-model="treeFilter" placeholder="Filter..." />
    </div>

    <div class="tree-body">
      <!-- Empty state -->
      <div v-if="!connStore.connections.length" class="tree-empty">
        <span>No connections yet</span>
        <button class="btn-primary" style="margin-top: 8px; font-size: 11px;" @click="openNew">Add connection</button>
      </div>

      <!-- Grouped connections -->
      <template v-for="(conns, g) in groupedConnections.groups" :key="String(g)">
        <template v-if="!q || conns.some(connMatchesFilter)">
          <div class="tree-node tree-group" @click="toggleGroup(String(g))">
            <component :is="expandedGroups.has(String(g)) ? ChevronDown : ChevronRight" class="h-3 w-3 shrink-0 text-muted-foreground" />
            <FolderOpen class="h-3.5 w-3.5 text-muted-foreground shrink-0" />
            <span class="tree-label group-label">{{ g }}</span>
            <span class="tree-badge">{{ conns.length }}</span>
          </div>
          <template v-if="expandedGroups.has(String(g)) || !!q">
            <template v-for="conn in conns" :key="conn.id">
              <div v-if="connMatchesFilter(conn)" class="tree-section" style="padding-left:12px">
                <div class="tree-node tree-conn" :data-conn-id="conn.id" :class="{ active: connStore.activeConn?.id === conn.id }" @click="onToggleConn(conn)">
                  <component :is="connStore.tree[conn.id]?.expanded ? ChevronDown : ChevronRight" class="h-3 w-3 shrink-0 text-muted-foreground" />
                  <Server class="h-3.5 w-3.5 text-primary shrink-0" />
                  <span class="tree-label">{{ conn.name }}</span>
                  <span class="tree-actions">
                    <button class="btn-icon tree-action" title="Edit" @click.stop="openEdit(conn)"><Pencil class="h-3 w-3" /></button>
                    <button class="btn-icon tree-action" title="Delete" @click.stop="remove(conn.id, $event)"><Trash2 class="h-3 w-3" /></button>
                  </span>
                </div>
                <template v-if="connStore.tree[conn.id]?.expanded">
                  <div v-if="!connStore.tree[conn.id].databases.length" class="tree-node tree-loading-msg" style="padding-left:40px">
                    <template v-if="loadingConns.has(conn.id)"><span class="loading-dot" />Loading...</template>
                    <template v-else>No databases</template>
                  </div>
                  <div v-for="db in filterDbs(connStore.tree[conn.id].databases)" :key="db" class="tree-section">
                    <div class="tree-node tree-db" :class="{ active: connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 32px" @click="onToggleDb(conn, db)" @contextmenu="openCtxMenu($event, 'db', conn, db, '')">
                      <component :is="connStore.tree[conn.id]?.expandedDbs[db] ? ChevronDown : ChevronRight" class="h-3 w-3 shrink-0 text-muted-foreground" />
                      <Database class="h-3.5 w-3.5 text-ferango-blue shrink-0" />
                      <span class="tree-label">{{ db }}</span>
                    </div>
                    <template v-if="connStore.tree[conn.id]?.expandedDbs[db]">
                      <div v-if="!filterCols(connStore.tree[conn.id].expandedDbs[db]).length" class="tree-node tree-empty-msg" style="padding-left:60px">No collections</div>
                      <div v-for="col in filterCols(connStore.tree[conn.id].expandedDbs[db])" :key="col" class="tree-node tree-col" :class="{ active: connStore.activeCollection === col && connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 52px" @click="onSelectCol(conn, db, col)" @contextmenu="openCtxMenu($event, 'col', conn, db, col)">
                        <Table2 class="h-3.5 w-3.5 text-primary shrink-0" />
                        <span class="tree-label">{{ col }}</span>
                      </div>
                    </template>
                  </div>
                </template>
              </div>
            </template>
          </template>
        </template>
      </template>

      <!-- Ungrouped connections -->
      <template v-for="conn in groupedConnections.ungrouped" :key="conn.id">
        <div v-if="connMatchesFilter(conn)" class="tree-section">
          <div class="tree-node tree-conn" :data-conn-id="conn.id" :class="{ active: connStore.activeConn?.id === conn.id }" @click="onToggleConn(conn)">
            <component :is="connStore.tree[conn.id]?.expanded ? ChevronDown : ChevronRight" class="h-3 w-3 shrink-0 text-muted-foreground" />
            <Server class="h-3.5 w-3.5 text-primary shrink-0" />
            <span class="tree-label">{{ conn.name }}</span>
            <span class="tree-actions">
              <button class="btn-icon tree-action" title="Edit" @click.stop="openEdit(conn)">✎</button>
              <button class="btn-icon tree-action" title="Delete" @click.stop="remove(conn.id, $event)">✕</button>
            </span>
          </div>
          <template v-if="connStore.tree[conn.id]?.expanded">
            <div v-if="!connStore.tree[conn.id].databases.length" class="tree-node tree-loading-msg" style="padding-left:28px">
              <template v-if="loadingConns.has(conn.id)"><span class="loading-dot" />Loading...</template>
              <template v-else>No databases</template>
            </div>
            <div v-for="db in filterDbs(connStore.tree[conn.id].databases)" :key="db" class="tree-section">
              <div class="tree-node tree-db" :class="{ active: connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 20px" @click="onToggleDb(conn, db)" @contextmenu="openCtxMenu($event, 'db', conn, db, '')">
                <component :is="connStore.tree[conn.id]?.expandedDbs[db] ? ChevronDown : ChevronRight" class="h-3 w-3 shrink-0 text-muted-foreground" />
                <Database class="h-3.5 w-3.5 text-ferango-blue shrink-0" />
                <span class="tree-label">{{ db }}</span>
              </div>
              <template v-if="connStore.tree[conn.id]?.expandedDbs[db]">
                <div v-if="!filterCols(connStore.tree[conn.id].expandedDbs[db]).length" class="tree-node tree-empty-msg" style="padding-left:48px">No collections</div>
                <div v-for="col in filterCols(connStore.tree[conn.id].expandedDbs[db])" :key="col" class="tree-node tree-col" :class="{ active: connStore.activeCollection === col && connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 40px" @click="onSelectCol(conn, db, col)" @contextmenu="openCtxMenu($event, 'col', conn, db, col)">
                  <Table2 class="h-3.5 w-3.5 text-primary shrink-0" />
                  <span class="tree-label">{{ col }}</span>
                </div>
              </template>
            </div>
          </template>
        </div>
      </template>
    </div>
  </div>

  <!-- Connection form modal -->
  <Dialog :open="showModal" @update:open="showModal = $event">
    <DialogContent class="sm:max-w-[520px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">{{ editingConn ? 'Edit Connection' : 'New Connection' }}</DialogTitle>
      </DialogHeader>

      <div class="form-row">
        <div class="form-group" style="flex:2">
          <label>Name</label>
          <input v-model="form.name" placeholder="My MongoDB" />
        </div>
        <div class="form-group" style="flex:1">
          <label>Group</label>
          <input v-model="form.group" placeholder="(optional)" list="groups-list" />
          <datalist id="groups-list">
            <option v-for="g in allGroups" :key="g" :value="g" />
          </datalist>
        </div>
      </div>

      <div class="mode-tabs">
        <button :class="['mode-tab', { active: mode === 'params' }]" @click="mode = 'params'">Parameters</button>
        <button :class="['mode-tab', { active: mode === 'uri' }]" @click="mode = 'uri'">URI</button>
      </div>

      <template v-if="mode === 'params'">
        <div class="form-row">
          <div class="form-group" style="flex:2">
            <label>Host</label>
            <input v-model="form.host" placeholder="localhost" />
          </div>
          <div class="form-group" style="flex:1">
            <label>Port</label>
            <input v-model="form.port" placeholder="27017" />
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label>Username</label>
            <input v-model="form.user" placeholder="(optional)" />
          </div>
          <div class="form-group">
            <label>Password</label>
            <input v-model="form.password" type="password" placeholder="(optional)" />
          </div>
        </div>
        <div class="form-group">
          <label>Options</label>
          <input v-model="form.options" placeholder="authSource=admin&tls=true" />
        </div>
      </template>

      <template v-else>
        <div class="form-group">
          <label>Connection URI</label>
          <input v-model="form.uri" placeholder="mongodb+srv://user:pass@cluster.mongodb.net" />
        </div>
      </template>

      <!-- SSH Tunnel section -->
      <div class="ssh-toggle" @click="showSsh = !showSsh">
        <span class="ssh-caret">{{ showSsh ? '▾' : '▸' }}</span>
        <span>SSH Tunnel</span>
        <span v-if="form.ssh_host" class="ssh-active-dot" title="Tunnel configured" />
      </div>
      <template v-if="showSsh">
        <div class="form-row">
          <div class="form-group" style="flex:3">
            <label>SSH Host</label>
            <input v-model="form.ssh_host" placeholder="bastion.example.com" />
          </div>
          <div class="form-group" style="flex:1">
            <label>Port</label>
            <input v-model="form.ssh_port" placeholder="22" />
          </div>
        </div>
        <div class="form-group">
          <label>SSH User</label>
          <input v-model="form.ssh_user" placeholder="ubuntu" />
        </div>
        <div class="form-group">
          <label>SSH Key Path</label>
          <input v-model="form.ssh_key_path" placeholder="~/.ssh/id_rsa (leave empty to use SSH agent)" />
        </div>
      </template>

      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" :disabled="testing" @click="testConn">
          {{ testing ? 'Testing...' : 'Test' }}
        </Button>
        <Button variant="outline" size="sm" @click="showModal = false">Cancel</Button>
        <Button size="sm" :disabled="saving" @click="save">
          {{ saving ? 'Saving...' : 'Save' }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Context menu -->
  <Teleport to="body">
    <template v-if="ctxMenu.show">
      <div class="fixed inset-0 z-50" @click.stop="closeCtxMenu" @contextmenu.prevent="closeCtxMenu" />
      <div class="fixed z-50 min-w-[180px] rounded-md border border-border bg-popover p-1 shadow-lg" :style="{ top: ctxMenu.y + 'px', left: ctxMenu.x + 'px' }" @click.stop>

        <!-- DB-level menu -->
        <template v-if="ctxMenu.type === 'db'">
          <div class="px-2 py-1.5 text-[10px] font-semibold text-muted-foreground uppercase tracking-wide border-b border-border mb-0.5 overflow-hidden text-ellipsis whitespace-nowrap">{{ ctxMenu.db }}</div>
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxCreateCollection">
            <Plus class="h-3.5 w-3.5 shrink-0" /> Create Collection
          </button>
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxCopyUri">
            <Copy class="h-3.5 w-3.5 shrink-0" /> Copy URI
          </button>
          <div class="h-px bg-border my-1 mx-1" />
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-destructive/10 hover:text-destructive w-full text-left text-destructive" @click="ctxDropDatabase">
            <Trash2 class="h-3.5 w-3.5 shrink-0" /> Drop Database
          </button>
        </template>

        <!-- Collection-level menu -->
        <template v-else>
          <div class="px-2 py-1.5 text-[10px] font-semibold text-muted-foreground uppercase tracking-wide border-b border-border mb-0.5 overflow-hidden text-ellipsis whitespace-nowrap">{{ ctxMenu.col }}</div>
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxFindAll">
            <Search class="h-3.5 w-3.5 shrink-0" /> Find all documents
          </button>
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxInsertDoc">
            <Plus class="h-3.5 w-3.5 shrink-0" /> Insert document
          </button>
          <div class="h-px bg-border my-1 mx-1" />
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxViewStats">
            <BarChart3 class="h-3.5 w-3.5 shrink-0" /> Collection stats
          </button>
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxViewIndexes">
            <ListTree class="h-3.5 w-3.5 shrink-0" /> View indexes
          </button>
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxInferSchema">
            <Braces class="h-3.5 w-3.5 shrink-0" /> Infer schema
          </button>
          <div class="h-px bg-border my-1 mx-1" />
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxNewScript">
            <Plus class="h-3.5 w-3.5 shrink-0" /> New Script
          </button>
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left disabled:opacity-40 disabled:cursor-default disabled:hover:bg-transparent" :disabled="!lastScriptForCtx" @click="ctxOpenLastScript">
            <FileText class="h-3.5 w-3.5 shrink-0" /> Open Last Script{{ lastScriptForCtx ? ` (${lastScriptForCtx.name})` : '' }}
          </button>
          <div class="h-px bg-border my-1 mx-1" />
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-accent hover:text-accent-foreground w-full text-left" @click="ctxCopyUri">
            <Copy class="h-3.5 w-3.5 shrink-0" /> Copy URI
          </button>
          <div class="h-px bg-border my-1 mx-1" />
          <button class="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-pointer hover:bg-destructive/10 hover:text-destructive w-full text-left text-destructive" @click="ctxDropCollection">
            <Trash2 class="h-3.5 w-3.5 shrink-0" /> Drop Collection
          </button>
        </template>

      </div>
    </template>
  </Teleport>

  <!-- Confirm dialog -->
  <Dialog :open="confirmDlg.show" @update:open="(v: boolean) => { if (!v) cancelConfirm() }">
    <DialogContent class="sm:max-w-[360px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Confirm</DialogTitle>
      </DialogHeader>
      <p class="text-[13px] text-foreground leading-relaxed py-1">{{ confirmDlg.message }}</p>
      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" @click="cancelConfirm">Cancel</Button>
        <Button variant="destructive" size="sm" @click="doConfirm">Confirm</Button>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Import connections dialog -->
  <Dialog :open="importDlg.show" @update:open="(v: boolean) => { importDlg.show = v }">
    <DialogContent class="sm:max-w-[480px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Import Connections</DialogTitle>
      </DialogHeader>
      <p class="text-[13px] text-foreground mb-2">Select connections to import:</p>
      <div class="import-list">
        <label
          v-for="(c, i) in importDlg.candidates"
          :key="i"
          class="import-row"
        >
          <input type="checkbox" v-model="c.selected" />
          <span class="import-name">{{ c.name }}</span>
          <span class="import-uri">{{ c.uri || `${c.host}:${c.port}` }}</span>
        </label>
      </div>
      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" @click="importDlg.show = false">Cancel</Button>
        <Button size="sm" :disabled="!importDlg.candidates.some(c => c.selected)" @click="doImport">
          Import {{ importDlg.candidates.filter(c => c.selected).length }} connection(s)
        </Button>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Collection stats dialog -->
  <Dialog :open="statsDlg.show" @update:open="(v: boolean) => { statsDlg.show = v }">
    <DialogContent class="sm:max-w-[360px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Collection Stats</DialogTitle>
      </DialogHeader>
      <div v-if="statsDlg.loading" class="dlg-loading">Loading...</div>
      <table v-else-if="statsDlg.stats" class="stats-table">
        <thead><tr><th>Property</th><th>Value</th></tr></thead>
        <tbody>
          <tr><td class="stats-label">Namespace</td><td class="stats-val">{{ statsDlg.stats.ns }}</td></tr>
          <tr><td class="stats-label">Documents</td><td class="stats-val">{{ statsDlg.stats.count.toLocaleString() }}</td></tr>
          <tr><td class="stats-label">Storage size</td><td class="stats-val">{{ formatBytes(statsDlg.stats.storage_size) }}</td></tr>
          <tr><td class="stats-label">Avg object size</td><td class="stats-val">{{ formatBytes(statsDlg.stats.avg_obj_size) }}</td></tr>
          <tr><td class="stats-label">Total index size</td><td class="stats-val">{{ formatBytes(statsDlg.stats.total_index_size) }}</td></tr>
          <tr><td class="stats-label">Indexes</td><td class="stats-val">{{ statsDlg.stats.index_count }}</td></tr>
        </tbody>
      </table>
      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" @click="statsDlg.show = false">Close</Button>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Indexes dialog -->
  <Dialog :open="indexesDlg.show" @update:open="(v: boolean) => { indexesDlg.show = v }">
    <DialogContent class="sm:max-w-[620px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Indexes — {{ indexesDlg.col }}</DialogTitle>
      </DialogHeader>
      <div v-if="indexesDlg.loading" class="dlg-loading">Loading...</div>
      <div v-else-if="!indexesDlg.indexes.length" class="dlg-loading">No indexes found</div>
      <div v-else class="scroll-body">
        <table class="info-table">
          <thead><tr><th>Name</th><th>Keys</th><th>Unique</th><th>Sparse</th></tr></thead>
          <tbody>
            <tr v-for="idx in indexesDlg.indexes" :key="idx.name">
              <td class="idx-name">{{ idx.name }}</td>
              <td class="idx-keys">{{ JSON.stringify(idx.keys) }}</td>
              <td class="info-flag">{{ idx.unique ? '✓' : '' }}</td>
              <td class="info-flag">{{ idx.sparse ? '✓' : '' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" @click="indexesDlg.show = false">Close</Button>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Schema inference dialog -->
  <Dialog :open="schemaDlg.show" @update:open="(v: boolean) => { schemaDlg.show = v }">
    <DialogContent class="sm:max-w-[580px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Schema — {{ schemaDlg.col }}</DialogTitle>
      </DialogHeader>
      <div v-if="schemaDlg.loading" class="dlg-loading">Sampling 100 documents...</div>
      <div v-else-if="!schemaDlg.fields.length" class="dlg-loading">No fields found</div>
      <div v-else class="scroll-body">
        <table class="info-table">
          <thead><tr><th>Field</th><th>Types</th><th>Presence</th></tr></thead>
          <tbody>
            <tr v-for="f in schemaDlg.fields" :key="f.path">
              <td class="schema-field">{{ f.path }}</td>
              <td class="schema-types">{{ f.types.join(', ') }}</td>
              <td class="schema-pct">{{ (f.presence * 100).toFixed(0) }}%</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" @click="schemaDlg.show = false">Close</Button>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Insert document dialog -->
  <Dialog :open="insertDlg.show" @update:open="(v: boolean) => { insertDlg.show = v }">
    <DialogContent class="sm:max-w-[520px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Insert Document — {{ insertDlg.col }}</DialogTitle>
      </DialogHeader>
      <div class="form-group">
        <label for="insert-doc-json">Document JSON</label>
        <textarea
          id="insert-doc-json"
          v-model="insertDlg.json"
          class="insert-textarea"
          spellcheck="false"
          @keydown.ctrl.enter.prevent="submitInsert"
          @keydown.meta.enter.prevent="submitInsert"
        />
      </div>
      <div v-if="insertDlg.error" class="insert-error">{{ insertDlg.error }}</div>
      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" @click="insertDlg.show = false">Cancel</Button>
        <Button size="sm" @click="submitInsert">Insert</Button>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Create collection dialog -->
  <Dialog :open="newColDlg.show" @update:open="(v: boolean) => { newColDlg.show = v }">
    <DialogContent class="sm:max-w-[360px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">New Collection</DialogTitle>
      </DialogHeader>
      <p class="text-[13px] text-muted-foreground">Database: <strong>{{ newColDlg.db }}</strong></p>
      <div class="form-group">
        <input
          v-model="newColDlg.name"
          placeholder="collection_name"
          autofocus
          @keyup.enter="submitNewCol"
          @keyup.escape="newColDlg.show = false"
        />
      </div>
      <div class="flex justify-end gap-2 pt-4 border-t border-border">
        <Button variant="outline" size="sm" @click="newColDlg.show = false">Cancel</Button>
        <Button size="sm" :disabled="!newColDlg.name.trim()" @click="submitNewCol">Create</Button>
      </div>
    </DialogContent>
  </Dialog>
</template>

<style scoped>
.tree-root { display: flex; flex-direction: column; height: 50%; min-height: 120px; overflow: hidden; }
.tree-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 6px 8px; font-size: 11px; font-weight: 600;
  color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px;
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.tree-title { user-select: none; }
.tree-header-actions { display: flex; gap: 1px; }
.tree-search { padding: 4px 6px; flex-shrink: 0; }
.tree-search input { font-size: 11px; padding: 3px 6px; width: 100%; }
.tree-body { flex: 1; overflow-y: auto; padding: 4px 0; }
.tree-empty {
  display: flex; flex-direction: column; align-items: center;
  padding: 20px 10px; color: var(--text-muted); font-size: 11px; text-align: center;
}

.tree-node {
  display: flex; align-items: center; gap: 4px;
  padding: 3px 8px; cursor: pointer; border-radius: 0;
  user-select: none; white-space: nowrap; overflow: hidden;
}
.tree-node:hover { background: var(--bg-hover); }
.tree-node.active { background: var(--bg-active); }
.tree-caret { font-size: 9px; color: var(--text-muted); width: 10px; flex-shrink: 0; }
.tree-icon { font-size: 12px; flex-shrink: 0; }
.tree-conn .tree-icon { color: var(--accent); }
.db-icon  { color: var(--yellow); }
.col-icon { color: var(--blue); }
.tree-label { flex: 1; overflow: hidden; text-overflow: ellipsis; font-size: 12px; color: var(--text); }
.tree-node.active .tree-label { color: var(--accent); }
.tree-empty-msg { font-size: 11px; color: var(--text-muted); padding: 2px 8px; cursor: default; }

.tree-actions { display: none; gap: 2px; margin-left: auto; }
.tree-node:hover .tree-actions { display: flex; }
.tree-action { font-size: 11px; padding: 1px 4px; }

/* Loading indicator */
.tree-loading-msg { font-size: 11px; color: var(--text-muted); cursor: default; gap: 6px; }
.loading-dot {
  display: inline-block; width: 6px; height: 6px;
  border-radius: 50%; background: var(--accent);
  animation: blink 1s ease-in-out infinite;
}
@keyframes blink { 0%, 100% { opacity: 0.2; } 50% { opacity: 1; } }

/* Mode tabs */
.mode-tabs { display: flex; gap: 0; margin-bottom: 12px; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
.mode-tab { flex: 1; background: transparent; color: var(--text-dim); border-radius: 0; padding: 5px; font-size: 11px; border: none; }
.mode-tab.active { background: var(--bg-active); color: var(--accent); }
.mode-tab:hover:not(.active) { background: var(--bg-hover); }

/* Import dialog */
.import-list { display: flex; flex-direction: column; gap: 4px; max-height: 280px; overflow-y: auto; margin-bottom: 12px; }
.import-row {
  display: flex; align-items: center; gap: 8px;
  padding: 5px 8px; border-radius: var(--radius); cursor: pointer;
  border: 1px solid var(--border);
}
.import-row:hover { background: var(--bg-hover); }
.import-name { font-size: 12px; color: var(--text); flex-shrink: 0; }
.import-uri { font-size: 10px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; text-align: right; }

/* Connection groups */
.tree-group {
  font-size: 11px; font-weight: 600; text-transform: uppercase;
  letter-spacing: 0.5px; color: var(--text-muted);
}
.group-icon { color: var(--text-muted) !important; }
.group-label { color: var(--text-muted); }
.tree-badge {
  margin-left: auto; font-size: 10px; padding: 1px 5px;
  background: var(--bg-hover); color: var(--text-muted);
  border-radius: 8px;
}

/* Stats / indexes / schema / insert modals */
.dlg-loading { padding: 16px 0; font-size: 12px; color: var(--text-muted); text-align: center; }
.stats-table { width: 100%; border-collapse: collapse; font-size: 12px; margin-bottom: 4px; }
.stats-table thead tr { border-bottom: 1px solid var(--border); }
.stats-table th { text-align: left; font-size: 10px; color: var(--text-muted); text-transform: uppercase; padding: 0 0 6px; font-weight: 600; }
.stats-label { color: var(--text-dim); padding: 5px 12px 5px 0; white-space: nowrap; }
.stats-val { color: var(--text); font-family: var(--font-mono); font-size: 12px; }
.scroll-body { max-height: 320px; overflow-y: auto; margin-bottom: 4px; }
.info-table { width: 100%; border-collapse: collapse; font-size: 12px; }
.info-table thead tr { border-bottom: 1px solid var(--border); position: sticky; top: 0; background: var(--bg-card); }
.info-table th { text-align: left; font-size: 10px; color: var(--text-muted); text-transform: uppercase; padding: 0 12px 6px 0; font-weight: 600; }
.info-table td { padding: 5px 12px 5px 0; color: var(--text-dim); vertical-align: top; }
.idx-name { color: var(--accent); font-family: var(--font-mono); font-size: 11px; white-space: nowrap; }
.idx-keys { color: var(--text); font-family: var(--font-mono); font-size: 11px; word-break: break-all; }
.info-flag { color: var(--green); text-align: center; }
.schema-field { color: var(--accent); font-family: var(--font-mono); font-size: 11px; white-space: nowrap; }
.schema-types { color: var(--text); font-size: 11px; }
.schema-pct { color: var(--text-dim); text-align: right; font-size: 11px; white-space: nowrap; padding-right: 0; }
.insert-textarea {
  width: 100%; min-height: 160px; resize: vertical;
  font-family: var(--font-mono); font-size: 12px;
  background: var(--bg-input); color: var(--text);
  border: 1px solid var(--border); border-radius: var(--radius);
  padding: 8px; outline: none; line-height: 1.5;
}
.insert-textarea:focus { border-color: var(--accent); }
.insert-error { font-size: 11px; color: var(--red); margin-top: 6px; word-break: break-word; }

/* SSH tunnel toggle */
.ssh-toggle {
  display: flex; align-items: center; gap: 6px;
  padding: 6px 0; font-size: 11px; color: var(--text-dim);
  cursor: pointer; user-select: none; margin-bottom: 4px;
  border-top: 1px solid var(--border);
}
.ssh-toggle:hover { color: var(--text); }
.ssh-caret { font-size: 9px; color: var(--text-muted); }
.ssh-active-dot {
  width: 6px; height: 6px; border-radius: 50%;
  background: var(--accent); margin-left: auto;
}
</style>
