<script setup lang="ts">
import { ref, computed, inject, watch, nextTick } from 'vue'
import { useEditorStore } from '../stores/editor'
import { useConnectionsStore } from '../stores/connections'
import type { ScriptFile, HistoryEntry } from '../lib/tauri'
import { listHistory, clearHistory, exportScriptsZip } from '../lib/tauri'

const editorStore = useEditorStore()
const connStore = useConnectionsStore()
const toast = inject<(msg: string, type?: 'success' | 'error' | 'info') => void>('toast')!

const search = ref('')
const expandedFolders = ref<Set<string>>(new Set())

// Group scripts by folder
const grouped = computed(() => {
  const map: Record<string, ScriptFile[]> = {}
  for (const s of editorStore.allScripts) {
    const q = search.value.toLowerCase()
    if (q && !s.name.toLowerCase().includes(q) && !s.folder.toLowerCase().includes(q)) continue
    if (!map[s.folder]) map[s.folder] = []
    map[s.folder].push(s)
  }
  return map
})

function toggleFolder(folder: string) {
  if (expandedFolders.value.has(folder)) expandedFolders.value.delete(folder)
  else expandedFolders.value.add(folder)
  expandedFolders.value = new Set(expandedFolders.value)
}

async function openScript(script: ScriptFile) {
  try {
    await editorStore.openScript(script)
    // Auto-activate the connection this script belongs to
    const conn = connStore.connections.find(c => c.name === script.folder)
    if (!conn) return

    // Ensure the connection is expanded (connected and databases fetched)
    const node = connStore.tree[conn.id]
    if (!node?.expanded) {
      await connStore.toggleConnection(conn)
    } else {
      connStore.activeConn = conn
    }

    // If editorStore.openScript already restored a saved context for this script,
    // trust it (more specific than the last-used heuristic).
    const tab = editorStore.activeTab()
    const hasSavedCtx = !!(tab?.connId && tab.connId === conn.id && tab.dbName)

    // Determine the DB to activate: saved context > last used > first available
    if (!hasSavedCtx) {
      const lastDb = connStore.lastDbPerConn[conn.id]
      const databases = connStore.tree[conn.id]?.databases ?? []
      const targetDb = lastDb && databases.includes(lastDb) ? lastDb : databases[0]
      if (targetDb && connStore.activeDb !== targetDb) {
        await connStore.activateDatabase(conn, targetDb)
      }
    }

    // Try to extract collection name from the script content and auto-select it
    const content = tab?.content ?? ''
    const colMatch = content.match(/db\.getCollection\(["']([^"']+)["']\)/) ??
                     content.match(/db\.([a-zA-Z_$][\w$]*)\.(?:find|findOne|aggregate|count|insert|update|delete|drop)/)
    if (colMatch) {
      const colName = colMatch[1]
      const dbName = connStore.activeDb
      const cols = connStore.tree[conn.id]?.expandedDbs[dbName] ?? []
      if (cols.includes(colName)) {
        connStore.selectCollection(conn, dbName, colName)
      }
    }

    // Scroll the connection tree to make the active connection visible
    await nextTick()
    const el = document.querySelector(`[data-conn-id="${conn.id}"]`)
    el?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  } catch (e) { toast(String(e), 'error') }
}

async function newScript() {
  const folder = connStore.activeConn?.name ?? 'General'
  await editorStore.newScript(folder)
  await editorStore.refreshScripts()
  expandedFolders.value.add(folder)
  expandedFolders.value = new Set(expandedFolders.value)
}

const renamingPath = ref<string | null>(null)
const renameVal = ref('')
function startRename(script: ScriptFile, e: Event) {
  e.stopPropagation()
  renamingPath.value = script.path
  renameVal.value = script.name
}
async function commitRename(e: Event) {
  e.stopPropagation()
  if (!renamingPath.value || !renameVal.value.trim()) { renamingPath.value = null; return }
  try {
    await editorStore.renameActive(renameVal.value)
    // If not active tab, rename directly
    const idx = editorStore.tabs.findIndex(t => t.script.path === renamingPath.value)
    if (idx < 0) {
      // rename via store
      const { renameScript } = await import('../lib/tauri')
      await renameScript(renamingPath.value, renameVal.value)
      await editorStore.refreshScripts()
    }
    toast('Renamed', 'success')
  } catch (err) { toast(String(err), 'error') }
  renamingPath.value = null
}

async function deleteScript(script: ScriptFile, e: Event) {
  e.stopPropagation()
  if (!confirm(`Delete "${script.name}"?`)) return
  try {
    await editorStore.removeScript(script.path)
    toast('Script deleted', 'info')
  } catch (err) { toast(String(err), 'error') }
}

// ── Export zip ────────────────────────────────────────────────────────────────
async function exportZip() {
  try {
    const path = await exportScriptsZip()
    toast(`Exported: ${path}`, 'success')
  } catch (err) { toast(String(err), 'error') }
}

// ── History ───────────────────────────────────────────────────────────────────
const historyOpen = ref(false)
const historyEntries = ref<HistoryEntry[]>([])

async function loadHistory() {
  const connId = connStore.activeConn?.id
  if (!connId) { historyEntries.value = []; return }
  try { historyEntries.value = await listHistory(connId, 30) }
  catch { historyEntries.value = [] }
}

async function doClearHistory() {
  const connId = connStore.activeConn?.id
  if (!connId) return
  await clearHistory(connId)
  historyEntries.value = []
  toast('History cleared', 'info')
}

async function copyHistoryEntry(entry: HistoryEntry) {
  await navigator.clipboard.writeText(entry.query)
  toast(`Copied: ${entry.query.length > 40 ? entry.query.slice(0, 40) + '…' : entry.query}`, 'info')
}

watch(historyOpen, (open) => { if (open) loadHistory() })
watch(() => connStore.activeConn?.id, () => { if (historyOpen.value) loadHistory() })
</script>

<template>
  <div class="scripts-root">
    <div class="scripts-header">
      <span class="scripts-title">Scripts</span>
      <button class="btn-icon" title="Export all scripts as zip" @click="exportZip">↓</button>
      <button class="btn-icon" title="New script" @click="newScript">＋</button>
    </div>

    <div class="scripts-search">
      <input v-model="search" placeholder="Search scripts…" />
    </div>

    <div class="scripts-body">
      <div v-if="!Object.keys(grouped).length" class="scripts-empty">
        <span>No scripts yet</span>
      </div>

      <div v-for="(scripts, folder) in grouped" :key="folder" class="folder-section">
        <div class="folder-row" @click="toggleFolder(folder)">
          <span class="tree-caret">{{ expandedFolders.has(folder) ? '▾' : '▸' }}</span>
          <span class="folder-icon">📁</span>
          <span class="folder-name">{{ folder }}</span>
          <span class="folder-count">{{ scripts.length }}</span>
        </div>

        <template v-if="expandedFolders.has(folder)">
          <div
            v-for="script in scripts"
            :key="script.path"
            class="script-row"
            :class="{ active: editorStore.activeTab()?.script.path === script.path }"
            @click="openScript(script)"
          >
            <span class="script-icon">📄</span>
            <template v-if="renamingPath === script.path">
              <input
                v-model="renameVal"
                class="rename-input"
                @keyup.enter="commitRename"
                @keyup.escape="renamingPath = null"
                @blur="commitRename"
                @click.stop
                autofocus
              />
            </template>
            <span v-else class="script-name">{{ script.name }}</span>
            <span class="script-actions">
              <button class="btn-icon script-action" title="Rename" @click.stop="startRename(script, $event)">✎</button>
              <button class="btn-icon script-action danger" title="Delete" @click.stop="deleteScript(script, $event)">✕</button>
            </span>
          </div>
        </template>
      </div>
    </div>

    <!-- History section -->
    <div class="history-section">
      <div class="history-header" @click="historyOpen = !historyOpen">
        <span class="tree-caret">{{ historyOpen ? '▾' : '▸' }}</span>
        <span class="scripts-title">History</span>
        <button v-if="historyOpen && historyEntries.length" class="btn-icon" title="Clear history" @click.stop="doClearHistory">✕</button>
      </div>
      <template v-if="historyOpen">
        <div v-if="!connStore.activeConn" class="history-empty">Connect to a server first</div>
        <div v-else-if="!historyEntries.length" class="history-empty">No history yet</div>
        <div
          v-for="(entry, i) in historyEntries"
          :key="i"
          class="history-row"
          :title="entry.query"
          @click="copyHistoryEntry(entry)"
        >
          <span class="history-query">{{ entry.query.length > 45 ? entry.query.slice(0, 45) + '…' : entry.query }}</span>
          <span class="history-meta">{{ entry.db }} · {{ entry.elapsed_ms }}ms</span>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.scripts-root { display: flex; flex-direction: column; flex: 1; overflow: hidden; min-height: 80px; }
.scripts-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 6px 8px; font-size: 11px; font-weight: 600;
  color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px;
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.scripts-title { user-select: none; }
.scripts-search { padding: 4px 6px; flex-shrink: 0; }
.scripts-search input { font-size: 11px; padding: 3px 6px; }
.scripts-body { flex: 1; overflow-y: auto; padding: 2px 0; }
.scripts-empty { color: var(--text-muted); font-size: 11px; text-align: center; padding: 16px; }

.folder-row {
  display: flex; align-items: center; gap: 4px;
  padding: 3px 8px; cursor: pointer; user-select: none;
}
.folder-row:hover { background: var(--bg-hover); }
.tree-caret { font-size: 9px; color: var(--text-muted); width: 10px; }
.folder-icon { font-size: 12px; }
.folder-name { flex: 1; font-size: 12px; color: var(--text-dim); font-weight: 500; overflow: hidden; text-overflow: ellipsis; }
.folder-count { font-size: 10px; color: var(--text-muted); }

.script-row {
  display: flex; align-items: center; gap: 4px;
  padding: 2px 8px 2px 24px; cursor: pointer; border-radius: 0;
}
.script-row:hover { background: var(--bg-hover); }
.script-row.active { background: var(--bg-active); }
.script-icon { font-size: 11px; flex-shrink: 0; }
.script-name { flex: 1; font-size: 11px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); }
.script-row.active .script-name { color: var(--accent); }
.rename-input { flex: 1; font-size: 11px; padding: 1px 4px; font-family: var(--font-mono); }

.script-actions { display: none; gap: 1px; }
.script-row:hover .script-actions { display: flex; }
.script-action { font-size: 10px; padding: 1px 3px; }
.script-action.danger:hover { color: var(--red); }

/* History */
.history-section { flex-shrink: 0; border-top: 1px solid var(--border); }
.history-header {
  display: flex; align-items: center; gap: 4px;
  padding: 6px 8px; cursor: pointer; user-select: none;
}
.history-header:hover { background: var(--bg-hover); }
.history-empty { color: var(--text-muted); font-size: 11px; padding: 8px 12px; }
.history-row {
  padding: 4px 12px; cursor: pointer;
  border-bottom: 1px solid var(--border);
}
.history-row:hover { background: var(--bg-hover); }
.history-query { display: block; font-size: 11px; font-family: var(--font-mono); color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.history-meta { font-size: 10px; color: var(--text-muted); }
</style>
