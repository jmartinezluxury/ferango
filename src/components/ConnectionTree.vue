<script setup lang="ts">
import { ref, inject, computed, onMounted, onBeforeUnmount } from 'vue'
import { open as openFilePicker } from '@tauri-apps/plugin-dialog'
import { useConnectionsStore } from '../stores/connections'
import { useEditorStore } from '../stores/editor'
import type { ConnectionConfig } from '../lib/tauri'
import {
  createCollection, dropCollectionCmd, dropDatabaseCmd, parseCompassFile,
  getCollectionStats, inferSchema, listIndexesCmd, executeQuery,
} from '../lib/tauri'
import type { CollectionStats, SchemaField, IndexInfo } from '../lib/tauri'

const connStore = useConnectionsStore()
const editorStore = useEditorStore()
const toast = inject<(msg: string, type?: 'success' | 'error' | 'info') => void>('toast')!

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
}
function onSelectCol(conn: ConnectionConfig, db: string, col: string) {
  connStore.selectCollection(conn, db, col)
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
      <button class="btn-icon" title="Import from Compass" @click="openImport">↓</button>
      <button class="btn-icon" title="New connection" @click="openNew">＋</button>
    </div>

    <div class="tree-body">
      <!-- Empty state -->
      <div v-if="!connStore.connections.length" class="tree-empty">
        <span>No connections yet</span>
        <button class="btn-primary" style="margin-top: 8px; font-size: 11px;" @click="openNew">Add connection</button>
      </div>

      <!-- Grouped connections -->
      <template v-for="(conns, g) in groupedConnections.groups" :key="String(g)">
        <div class="tree-node tree-group" @click="toggleGroup(String(g))">
          <span class="tree-caret">{{ expandedGroups.has(String(g)) ? '▾' : '▸' }}</span>
          <span class="tree-icon group-icon">▣</span>
          <span class="tree-label group-label">{{ g }}</span>
          <span class="tree-badge">{{ conns.length }}</span>
        </div>
        <template v-if="expandedGroups.has(String(g))">
          <div v-for="conn in conns" :key="conn.id" class="tree-section" style="padding-left:12px">
            <div class="tree-node tree-conn" :class="{ active: connStore.activeConn?.id === conn.id }" @click="onToggleConn(conn)">
              <span class="tree-caret">{{ connStore.tree[conn.id]?.expanded ? '▾' : '▸' }}</span>
              <span class="tree-icon">⬡</span>
              <span class="tree-label">{{ conn.name }}</span>
              <span class="tree-actions">
                <button class="btn-icon tree-action" title="Edit" @click.stop="openEdit(conn)">✎</button>
                <button class="btn-icon tree-action" title="Delete" @click.stop="remove(conn.id, $event)">✕</button>
              </span>
            </div>
            <template v-if="connStore.tree[conn.id]?.expanded">
              <div v-if="!connStore.tree[conn.id].databases.length" class="tree-node tree-loading-msg" style="padding-left:40px">
                <template v-if="loadingConns.has(conn.id)"><span class="loading-dot" />Loading…</template>
                <template v-else>No databases</template>
              </div>
              <div v-for="db in connStore.tree[conn.id].databases" :key="db" class="tree-section">
                <div class="tree-node tree-db" :class="{ active: connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 32px" @click="onToggleDb(conn, db)" @contextmenu="openCtxMenu($event, 'db', conn, db, '')">
                  <span class="tree-caret">{{ connStore.tree[conn.id]?.expandedDbs[db] ? '▾' : '▸' }}</span>
                  <span class="tree-icon db-icon">◈</span>
                  <span class="tree-label">{{ db }}</span>
                </div>
                <template v-if="connStore.tree[conn.id]?.expandedDbs[db]">
                  <div v-if="!connStore.tree[conn.id].expandedDbs[db].length" class="tree-node tree-empty-msg" style="padding-left:60px">No collections</div>
                  <div v-for="col in connStore.tree[conn.id].expandedDbs[db]" :key="col" class="tree-node tree-col" :class="{ active: connStore.activeCollection === col && connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 52px" @click="onSelectCol(conn, db, col)" @contextmenu="openCtxMenu($event, 'col', conn, db, col)">
                    <span class="tree-icon col-icon">▤</span>
                    <span class="tree-label">{{ col }}</span>
                  </div>
                </template>
              </div>
            </template>
          </div>
        </template>
      </template>

      <!-- Ungrouped connections -->
      <div v-for="conn in groupedConnections.ungrouped" :key="conn.id" class="tree-section">
        <div class="tree-node tree-conn" :class="{ active: connStore.activeConn?.id === conn.id }" @click="onToggleConn(conn)">
          <span class="tree-caret">{{ connStore.tree[conn.id]?.expanded ? '▾' : '▸' }}</span>
          <span class="tree-icon">⬡</span>
          <span class="tree-label">{{ conn.name }}</span>
          <span class="tree-actions">
            <button class="btn-icon tree-action" title="Edit" @click.stop="openEdit(conn)">✎</button>
            <button class="btn-icon tree-action" title="Delete" @click.stop="remove(conn.id, $event)">✕</button>
          </span>
        </div>
        <template v-if="connStore.tree[conn.id]?.expanded">
          <div v-if="!connStore.tree[conn.id].databases.length" class="tree-node tree-loading-msg" style="padding-left:28px">
            <template v-if="loadingConns.has(conn.id)"><span class="loading-dot" />Loading…</template>
            <template v-else>No databases</template>
          </div>
          <div v-for="db in connStore.tree[conn.id].databases" :key="db" class="tree-section">
            <div class="tree-node tree-db" :class="{ active: connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 20px" @click="onToggleDb(conn, db)" @contextmenu="openCtxMenu($event, 'db', conn, db, '')">
              <span class="tree-caret">{{ connStore.tree[conn.id]?.expandedDbs[db] ? '▾' : '▸' }}</span>
              <span class="tree-icon db-icon">◈</span>
              <span class="tree-label">{{ db }}</span>
            </div>
            <template v-if="connStore.tree[conn.id]?.expandedDbs[db]">
              <div v-if="!connStore.tree[conn.id].expandedDbs[db].length" class="tree-node tree-empty-msg" style="padding-left:48px">No collections</div>
              <div v-for="col in connStore.tree[conn.id].expandedDbs[db]" :key="col" class="tree-node tree-col" :class="{ active: connStore.activeCollection === col && connStore.activeDb === db && connStore.activeConn?.id === conn.id }" style="padding-left: 40px" @click="onSelectCol(conn, db, col)" @contextmenu="openCtxMenu($event, 'col', conn, db, col)">
                <span class="tree-icon col-icon">▤</span>
                <span class="tree-label">{{ col }}</span>
              </div>
            </template>
          </div>
        </template>
      </div>
    </div>
  </div>

  <!-- Connection modal -->
  <div v-if="showModal" class="modal-overlay" @click.self="showModal = false">
    <div class="modal">
      <div class="modal-header">
        <span class="modal-title">{{ editingConn ? 'Edit Connection' : 'New Connection' }}</span>
        <button class="btn-icon" @click="showModal = false">✕</button>
      </div>

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

      <div class="modal-footer">
        <button class="btn-ghost" :disabled="testing" @click="testConn">
          {{ testing ? 'Testing…' : 'Test' }}
        </button>
        <button class="btn-ghost" @click="showModal = false">Cancel</button>
        <button class="btn-primary" :disabled="saving" @click="save">
          {{ saving ? 'Saving…' : 'Save' }}
        </button>
      </div>
    </div>
  </div>

  <!-- Context menu -->
  <Teleport to="body">
    <template v-if="ctxMenu.show">
      <div class="ctx-backdrop" @click.stop="closeCtxMenu" @contextmenu.prevent="closeCtxMenu" />
      <div class="ctx-menu" :style="{ top: ctxMenu.y + 'px', left: ctxMenu.x + 'px' }" @click.stop>

        <!-- DB-level menu -->
        <template v-if="ctxMenu.type === 'db'">
          <div class="ctx-header">{{ ctxMenu.db }}</div>
          <button class="ctx-item" @click="ctxCreateCollection">＋ Create Collection</button>
          <button class="ctx-item" @click="ctxCopyUri">⎘ Copy URI</button>
          <div class="ctx-sep" />
          <button class="ctx-item ctx-danger" @click="ctxDropDatabase">✕ Drop Database</button>
        </template>

        <!-- Collection-level menu -->
        <template v-else>
          <div class="ctx-header">{{ ctxMenu.col }}</div>
          <button class="ctx-item" @click="ctxFindAll">⊞ Find all documents</button>
          <button class="ctx-item" @click="ctxInsertDoc">＋ Insert document</button>
          <div class="ctx-sep" />
          <button class="ctx-item" @click="ctxViewStats">◎ Collection stats</button>
          <button class="ctx-item" @click="ctxViewIndexes">⊟ View indexes</button>
          <button class="ctx-item" @click="ctxInferSchema">∷ Infer schema</button>
          <div class="ctx-sep" />
          <button class="ctx-item" @click="ctxNewScript">＋ New Script</button>
          <button class="ctx-item" :disabled="!lastScriptForCtx" @click="ctxOpenLastScript">
            ↩ Open Last Script{{ lastScriptForCtx ? ` (${lastScriptForCtx.name})` : '' }}
          </button>
          <div class="ctx-sep" />
          <button class="ctx-item" @click="ctxCopyUri">⎘ Copy URI</button>
          <div class="ctx-sep" />
          <button class="ctx-item ctx-danger" @click="ctxDropCollection">✕ Drop Collection</button>
        </template>

      </div>
    </template>
  </Teleport>

  <!-- Confirm dialog -->
  <Teleport to="body">
    <div v-if="confirmDlg.show" class="modal-overlay" @click.self="cancelConfirm">
      <div class="modal modal-sm">
        <div class="modal-header">
          <span class="modal-title">Confirm</span>
          <button class="btn-icon" @click="cancelConfirm">✕</button>
        </div>
        <p class="confirm-msg">{{ confirmDlg.message }}</p>
        <div class="modal-footer">
          <button class="btn-ghost" @click="cancelConfirm">Cancel</button>
          <button class="btn-danger" @click="doConfirm">Confirm</button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Import connections dialog -->
  <Teleport to="body">
    <div v-if="importDlg.show" class="modal-overlay" @click.self="importDlg.show = false">
      <div class="modal" style="max-width:480px">
        <div class="modal-header">
          <span class="modal-title">Import Connections</span>
          <button class="btn-icon" @click="importDlg.show = false">✕</button>
        </div>
        <p class="confirm-msg" style="margin-bottom:8px">Select connections to import:</p>
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
        <div class="modal-footer">
          <button class="btn-ghost" @click="importDlg.show = false">Cancel</button>
          <button class="btn-primary" :disabled="!importDlg.candidates.some(c => c.selected)" @click="doImport">
            Import {{ importDlg.candidates.filter(c => c.selected).length }} connection(s)
          </button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Collection stats dialog -->
  <Teleport to="body">
    <div v-if="statsDlg.show" class="modal-overlay" @click.self="statsDlg.show = false">
      <div class="modal modal-sm">
        <div class="modal-header">
          <span class="modal-title">Collection Stats</span>
          <button class="btn-icon" @click="statsDlg.show = false">✕</button>
        </div>
        <div v-if="statsDlg.loading" class="dlg-loading">Loading…</div>
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
        <div class="modal-footer">
          <button class="btn-ghost" @click="statsDlg.show = false">Close</button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Indexes dialog -->
  <Teleport to="body">
    <div v-if="indexesDlg.show" class="modal-overlay" @click.self="indexesDlg.show = false">
      <div class="modal indexes-modal">
        <div class="modal-header">
          <span class="modal-title">Indexes — {{ indexesDlg.col }}</span>
          <button class="btn-icon" @click="indexesDlg.show = false">✕</button>
        </div>
        <div v-if="indexesDlg.loading" class="dlg-loading">Loading…</div>
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
        <div class="modal-footer">
          <button class="btn-ghost" @click="indexesDlg.show = false">Close</button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Schema inference dialog -->
  <Teleport to="body">
    <div v-if="schemaDlg.show" class="modal-overlay" @click.self="schemaDlg.show = false">
      <div class="modal schema-modal">
        <div class="modal-header">
          <span class="modal-title">Schema — {{ schemaDlg.col }}</span>
          <button class="btn-icon" @click="schemaDlg.show = false">✕</button>
        </div>
        <div v-if="schemaDlg.loading" class="dlg-loading">Sampling 100 documents…</div>
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
        <div class="modal-footer">
          <button class="btn-ghost" @click="schemaDlg.show = false">Close</button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Insert document dialog -->
  <Teleport to="body">
    <div v-if="insertDlg.show" class="modal-overlay" @click.self="insertDlg.show = false">
      <div class="modal insert-modal">
        <div class="modal-header">
          <span class="modal-title">Insert Document — {{ insertDlg.col }}</span>
          <button class="btn-icon" @click="insertDlg.show = false">✕</button>
        </div>
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
        <div class="modal-footer">
          <button class="btn-ghost" @click="insertDlg.show = false">Cancel</button>
          <button class="btn-primary" @click="submitInsert">Insert</button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Create collection dialog -->
  <Teleport to="body">
    <div v-if="newColDlg.show" class="modal-overlay" @click.self="newColDlg.show = false">
      <div class="modal modal-sm">
        <div class="modal-header">
          <span class="modal-title">New Collection</span>
          <button class="btn-icon" @click="newColDlg.show = false">✕</button>
        </div>
        <p class="confirm-msg" style="color: var(--text-muted)">Database: <strong>{{ newColDlg.db }}</strong></p>
        <div class="form-group">
          <input
            v-model="newColDlg.name"
            placeholder="collection_name"
            autofocus
            @keyup.enter="submitNewCol"
            @keyup.escape="newColDlg.show = false"
          />
        </div>
        <div class="modal-footer">
          <button class="btn-ghost" @click="newColDlg.show = false">Cancel</button>
          <button class="btn-primary" :disabled="!newColDlg.name.trim()" @click="submitNewCol">Create</button>
        </div>
      </div>
    </div>
  </Teleport>
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

/* Context menu */
.ctx-backdrop { position: fixed; inset: 0; z-index: 999; }
.ctx-menu {
  position: fixed; z-index: 1000;
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius); padding: 4px; min-width: 200px;
  box-shadow: 0 4px 16px rgba(0,0,0,0.5);
}
.ctx-header {
  padding: 4px 10px 6px; font-size: 10px; font-weight: 600;
  color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px;
  border-bottom: 1px solid var(--border); margin-bottom: 2px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.ctx-item {
  display: flex; width: 100%; align-items: center;
  padding: 6px 10px; font-size: 12px; color: var(--text);
  background: transparent; border: none; border-radius: 3px;
  text-align: left; cursor: pointer; white-space: nowrap;
  overflow: hidden; text-overflow: ellipsis;
}
.ctx-item:hover:not(:disabled) { background: var(--bg-hover); }
.ctx-item:disabled { opacity: 0.4; cursor: default; }
.ctx-danger { color: var(--red, #e06c75); }
.ctx-danger:hover:not(:disabled) { background: rgba(224, 108, 117, 0.12); }
.ctx-sep { height: 1px; background: var(--border); margin: 3px 6px; }

/* Loading indicator */
.tree-loading-msg { font-size: 11px; color: var(--text-muted); cursor: default; gap: 6px; }
.loading-dot {
  display: inline-block; width: 6px; height: 6px;
  border-radius: 50%; background: var(--accent);
  animation: blink 1s ease-in-out infinite;
}
@keyframes blink { 0%, 100% { opacity: 0.2; } 50% { opacity: 1; } }

/* Confirm / small modal */
.modal-sm { max-width: 360px; }
.confirm-msg { font-size: 13px; color: var(--text); padding: 8px 0 12px; line-height: 1.5; }
.btn-danger {
  background: var(--red, #e06c75); color: #fff; border: none;
  border-radius: var(--radius); padding: 6px 14px; font-size: 12px; cursor: pointer;
}
.btn-danger:hover { opacity: 0.85; }

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
.indexes-modal { min-width: 480px; max-width: 620px; }
.schema-modal  { min-width: 440px; max-width: 580px; }
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
.insert-modal { min-width: 400px; max-width: 520px; }
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
