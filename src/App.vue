<script setup lang="ts">
import { ref, onMounted, provide } from 'vue'
import { useConnectionsStore } from './stores/connections'
import { useEditorStore } from './stores/editor'
import { useSettingsStore } from './stores/settings'
import ConnectionTree from './components/ConnectionTree.vue'
import ScriptBrowser from './components/ScriptBrowser.vue'
import QueryEditor from './components/QueryEditor.vue'
import ResultViewer from './components/ResultViewer.vue'

const connStore = useConnectionsStore()
const editorStore = useEditorStore()
const settingsStore = useSettingsStore()

// ── Settings modal ────────────────────────────────────────────────────────────
const settingsOpen = ref(false)

// ── Shortcuts modal ───────────────────────────────────────────────────────────
const shortcutsOpen = ref(false)

interface Toast { id: number; msg: string; type: 'success' | 'error' | 'info' }
const toasts = ref<Toast[]>([])
let toastId = 0
function showToast(msg: string, type: Toast['type'] = 'info') {
  const id = ++toastId
  toasts.value.push({ id, msg, type })
  setTimeout(() => { toasts.value = toasts.value.filter(t => t.id !== id) }, 3000)
}
provide('toast', showToast)

const sidebarW = ref(260)
function startResizeH(e: MouseEvent) {
  const startX = e.clientX
  const startW = sidebarW.value
  const onMove = (e: MouseEvent) => { sidebarW.value = Math.max(180, Math.min(480, startW + e.clientX - startX)) }
  const onUp = () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp) }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

const editorH = ref(220)
function startResizeV(e: MouseEvent) {
  const startY = e.clientY
  const startH = editorH.value
  const onMove = (e: MouseEvent) => { editorH.value = Math.max(80, Math.min(600, startH + e.clientY - startY)) }
  const onUp = () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp) }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

onMounted(async () => {
  await settingsStore.init()
  await connStore.init()
  await editorStore.init()
})
</script>

<template>
  <div class="app-layout">
    <div class="toolbar">
      <span class="app-brand"><span class="brand-fer">Fer</span><span class="brand-ango">ango</span></span>
      <div class="toolbar-sep" />
      <span class="breadcrumb">
        <span v-if="connStore.activeConn" class="bc-item">{{ connStore.activeConn.name }}</span>
        <template v-if="connStore.activeDb">
          <span class="bc-sep">›</span>
          <span class="bc-item bc-db">{{ connStore.activeDb }}</span>
        </template>
        <template v-if="connStore.activeCollection">
          <span class="bc-sep">›</span>
          <span class="bc-item bc-col">{{ connStore.activeCollection }}</span>
        </template>
        <span v-if="!connStore.activeConn" class="bc-empty">No connection selected</span>
      </span>
      <span class="toolbar-spacer" />
      <button class="btn-icon toolbar-btn" title="Keyboard shortcuts" @click="shortcutsOpen = true">?</button>
      <button class="btn-icon toolbar-btn" title="Settings" @click="settingsOpen = true">⚙</button>
    </div>

    <div class="main-layout">
      <div class="sidebar" :style="{ width: sidebarW + 'px' }">
        <ConnectionTree />
        <div class="sidebar-divider" />
        <ScriptBrowser />
      </div>
      <div class="resize-h" @mousedown.prevent="startResizeH" />
      <div class="main-area">
        <div class="editor-wrap" :style="{ height: editorH + 'px' }">
          <QueryEditor />
        </div>
        <div class="resize-v" @mousedown.prevent="startResizeV" />
        <div class="viewer-wrap">
          <ResultViewer />
        </div>
      </div>
    </div>

    <div class="status-bar">
      <template v-if="editorStore.result">
        <span :class="editorStore.result.success ? 'status-ok' : 'status-err'">
          {{ editorStore.result.success ? `${editorStore.result.rows} docs` : 'Error' }}
        </span>
        <span class="status-sep"> · </span>
        <span>{{ editorStore.result.elapsed_ms }}ms</span>
      </template>
      <span v-else class="status-idle">Ready</span>
      <span class="status-spacer" />
      <span class="status-dir">{{ editorStore.scriptsDir }}</span>
    </div>
  </div>

  <!-- Settings modal -->
  <div v-if="settingsOpen" class="modal-overlay" @click.self="settingsOpen = false">
    <div class="modal settings-modal">
      <div class="modal-header">
        <span class="modal-title">Settings</span>
        <button class="btn-icon" @click="settingsOpen = false">✕</button>
      </div>

      <div class="settings-section">
        <div class="settings-label">Theme</div>
        <div class="theme-row">
          <button
            :class="['theme-btn', { active: settingsStore.theme === 'dark' }]"
            @click="settingsStore.setTheme('dark')"
          >Dark</button>
          <button
            :class="['theme-btn', { active: settingsStore.theme === 'light' }]"
            @click="settingsStore.setTheme('light')"
          >Light</button>
        </div>
      </div>

      <div class="settings-section">
        <div class="settings-label">Editor font size</div>
        <div class="font-size-row">
          <button class="btn-icon font-btn" :disabled="settingsStore.fontSize <= 10" @click="settingsStore.setFontSize(settingsStore.fontSize - 1)">−</button>
          <span class="font-size-val">{{ settingsStore.fontSize }}px</span>
          <button class="btn-icon font-btn" :disabled="settingsStore.fontSize >= 24" @click="settingsStore.setFontSize(settingsStore.fontSize + 1)">+</button>
        </div>
      </div>
    </div>
  </div>

  <!-- Shortcuts modal -->
  <div v-if="shortcutsOpen" class="modal-overlay" @click.self="shortcutsOpen = false">
    <div class="modal shortcuts-modal">
      <div class="modal-header">
        <span class="modal-title">Keyboard shortcuts</span>
        <button class="btn-icon" @click="shortcutsOpen = false">✕</button>
      </div>
      <table class="shortcuts-table">
        <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
        <tbody>
          <tr><td class="shortcut-key">Ctrl+Enter</td><td>Run statement at cursor</td></tr>
          <tr><td class="shortcut-key">Select + Ctrl+Enter</td><td>Run selected statements</td></tr>
          <tr><td class="shortcut-key">Run all button</td><td>Run all statements in file</td></tr>
          <tr><td class="shortcut-key">Shift+Alt+F</td><td>Format document</td></tr>
          <tr><td class="shortcut-key">Ctrl+S</td><td>Save script</td></tr>
          <tr><td class="shortcut-key">Ctrl+F</td><td>Find in editor</td></tr>
          <tr><td class="shortcut-key">Ctrl+Z</td><td>Undo</td></tr>
          <tr><td class="shortcut-key">Ctrl+Shift+Z</td><td>Redo</td></tr>
          <tr><td class="shortcut-key">Ctrl+/</td><td>Toggle line comment</td></tr>
        </tbody>
      </table>
    </div>
  </div>

  <div class="toast-container">
    <div v-for="t in toasts" :key="t.id" :class="['toast', t.type]">{{ t.msg }}</div>
  </div>
</template>

<style scoped>
.app-layout { display: flex; flex-direction: column; height: 100vh; overflow: hidden; }
.toolbar {
  height: var(--toolbar-h); background: var(--bg-sidebar);
  border-bottom: 1px solid var(--border);
  display: flex; align-items: center; padding: 0 12px; gap: 8px; flex-shrink: 0;
  -webkit-app-region: drag; user-select: none;
}
.toolbar * { -webkit-app-region: no-drag; }
.app-brand {
  font-size: 13px;
  font-weight: 800;
  letter-spacing: -0.3px;
  filter: drop-shadow(0 0 6px rgba(247,148,29,0.4));
}
.brand-fer  { color: #ffffff; }
.brand-ango { color: #f7941d; }
.toolbar-sep { width: 1px; height: 16px; background: var(--border); }
.breadcrumb { display: flex; align-items: center; gap: 5px; font-size: 12px; }
.bc-item  { color: var(--text-dim); }
.bc-db    { color: var(--blue); }
.bc-col   { color: var(--accent); }
.bc-sep   { color: var(--text-muted); }
.bc-empty { color: var(--text-muted); font-style: italic; }
.main-layout { display: flex; flex: 1; overflow: hidden; }
.sidebar {
  display: flex; flex-direction: column; background: var(--bg-sidebar);
  overflow: hidden; flex-shrink: 0; min-width: 180px;
}
.sidebar-divider { height: 1px; background: var(--border); flex-shrink: 0; }
.main-area { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
.editor-wrap { flex-shrink: 0; overflow: hidden; min-height: 80px; }
.viewer-wrap { flex: 1; overflow: hidden; }
.status-bar {
  height: var(--status-h); background: var(--bg-sidebar);
  border-top: 1px solid var(--border);
  display: flex; align-items: center; padding: 0 10px; gap: 6px;
  font-size: 11px; color: var(--text-dim); flex-shrink: 0;
}
.status-ok  { color: var(--green); }
.status-err { color: var(--red); }
.status-sep { color: var(--text-muted); }
.status-spacer { flex: 1; }
.status-idle { color: var(--text-muted); }
.status-dir { color: var(--text-muted); font-size: 10px; font-family: var(--font-mono); }

/* Toolbar right side */
.toolbar-spacer { flex: 1; }
.toolbar-btn { font-size: 16px; padding: 2px 8px; font-weight: 500; }

/* Settings modal */
.settings-modal { min-width: 300px; max-width: 360px; }
.settings-section { margin-bottom: 20px; }
.settings-label { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 8px; }
.theme-row { display: flex; gap: 8px; }
.theme-btn {
  flex: 1; padding: 6px; font-size: 12px;
  background: var(--bg-input); color: var(--text-dim);
  border: 1px solid var(--border); border-radius: var(--radius);
}
.theme-btn:hover { border-color: var(--accent); color: var(--text); }
.theme-btn.active { background: var(--accent); color: #0d1b2a; border-color: var(--accent); font-weight: 600; }
.font-size-row { display: flex; align-items: center; gap: 12px; }
.font-btn { font-size: 16px; width: 28px; height: 28px; display: flex; align-items: center; justify-content: center; border: 1px solid var(--border); }
.font-btn:disabled { opacity: 0.35; cursor: not-allowed; }
.font-size-val { font-size: 13px; color: var(--text); min-width: 36px; text-align: center; font-family: var(--font-mono); }

/* Shortcuts modal */
.shortcuts-modal { min-width: 380px; }
.shortcuts-table { width: 100%; border-collapse: collapse; font-size: 12px; }
.shortcuts-table thead tr { border-bottom: 1px solid var(--border); }
.shortcuts-table th { text-align: left; font-size: 10px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px; padding: 0 0 8px; font-weight: 600; }
.shortcuts-table td { padding: 5px 0; color: var(--text-dim); }
.shortcut-key {
  font-family: var(--font-mono); font-size: 11px;
  color: var(--accent); white-space: nowrap; padding-right: 20px;
}
</style>
