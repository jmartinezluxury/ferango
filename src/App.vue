<script setup lang="ts">
import { ref, watch, onMounted, provide } from 'vue'
import { useConnectionsStore } from './stores/connections'
import { useEditorStore } from './stores/editor'
import { useSettingsStore } from './stores/settings'
import { saveAiApiKey, getAiApiKeyExists, aiCheckHealth } from './lib/tauri'
import ConnectionTree from './components/ConnectionTree.vue'
import ScriptBrowser from './components/ScriptBrowser.vue'
import QueryEditor from './components/QueryEditor.vue'
import ResultViewer from './components/ResultViewer.vue'

const connStore = useConnectionsStore()
const editorStore = useEditorStore()
const settingsStore = useSettingsStore()

// ── Settings modal ────────────────────────────────────────────────────────────
const settingsOpen = ref(false)
const aiApiKey = ref('')
const aiKeyExists = ref(false)
const aiHealthStatus = ref<'idle' | 'checking' | 'ok' | 'fail'>('idle')

watch(settingsOpen, async (open) => {
  if (open) {
    aiApiKey.value = ''
    aiHealthStatus.value = 'idle'
    aiKeyExists.value = await getAiApiKeyExists(settingsStore.aiProvider).catch(() => false)
  }
})

watch(() => settingsStore.aiProvider, async () => {
  aiKeyExists.value = await getAiApiKeyExists(settingsStore.aiProvider).catch(() => false)
  aiApiKey.value = ''
  aiHealthStatus.value = 'idle'
})

async function saveApiKey() {
  if (!aiApiKey.value.trim()) return
  await saveAiApiKey(settingsStore.aiProvider, aiApiKey.value.trim())
  aiKeyExists.value = true
  aiApiKey.value = ''
  showToast('API key saved securely', 'success')
}

async function testAiConnection() {
  aiHealthStatus.value = 'checking'
  try {
    const ok = await aiCheckHealth()
    aiHealthStatus.value = ok ? 'ok' : 'fail'
    showToast(ok ? 'AI provider is reachable' : 'AI provider unreachable', ok ? 'success' : 'error')
  } catch {
    aiHealthStatus.value = 'fail'
    showToast('Failed to check AI provider', 'error')
  }
}

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

  // Disable the default browser context menu globally;
  // our custom context menus already call preventDefault() themselves.
  document.addEventListener('contextmenu', (e) => e.preventDefault())
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
        <!-- Tab bar -->
        <div v-if="editorStore.tabs.length > 0" class="tabs-bar">
          <div class="tabs-list">
            <div
              v-for="(tab, i) in editorStore.tabs"
              :key="tab.script.path"
              :class="['tab', { active: i === editorStore.activeTabIndex }]"
              @click="editorStore.switchTab(i)"
            >
              <span class="tab-name">{{ tab.script.name }}{{ tab.modified ? ' ●' : '' }}</span>
              <button class="tab-close" title="Close" @click.stop="editorStore.closeTab(i)">✕</button>
            </div>
          </div>
        </div>

        <!-- Workspace instances: one per tab, kept in DOM via v-show -->
        <div
          v-for="(tab, i) in editorStore.tabs"
          :key="tab.script.path"
          v-show="i === editorStore.activeTabIndex"
          class="workspace"
        >
          <div class="editor-wrap" :style="{ height: editorH + 'px' }">
            <QueryEditor :tabIndex="i" />
          </div>
          <div class="resize-v" @mousedown.prevent="startResizeV" />
          <div class="viewer-wrap">
            <ResultViewer :tabIndex="i" />
          </div>
        </div>

        <!-- Empty state when no tabs open -->
        <div v-if="editorStore.tabs.length === 0" class="workspace-empty">
          <span>Open a script or select a collection to get started</span>
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

      <div class="settings-divider" />

      <div class="settings-section">
        <div class="settings-label">AI Autocomplete</div>
        <label class="ai-toggle-row">
          <input type="checkbox" :checked="settingsStore.aiEnabled" @change="settingsStore.setAiEnabled(($event.target as HTMLInputElement).checked)" />
          <span>{{ settingsStore.aiEnabled ? 'Enabled' : 'Disabled' }}</span>
        </label>
      </div>

      <template v-if="settingsStore.aiEnabled">
        <div class="settings-section">
          <div class="settings-label">Provider</div>
          <div class="provider-row">
            <button
              v-for="p in (['ollama', 'openai', 'claude'] as const)"
              :key="p"
              :class="['theme-btn', { active: settingsStore.aiProvider === p }]"
              @click="settingsStore.setAiProvider(p)"
            >{{ p === 'ollama' ? 'Ollama' : p === 'openai' ? 'OpenAI' : 'Claude' }}</button>
          </div>
        </div>

        <div class="settings-section">
          <div class="settings-label">Endpoint</div>
          <input
            class="settings-input"
            :value="settingsStore.aiEndpoint"
            @change="settingsStore.setAiEndpoint(($event.target as HTMLInputElement).value)"
            placeholder="http://localhost:11434"
          />
        </div>

        <div class="settings-section">
          <div class="settings-label">Model</div>
          <input
            class="settings-input"
            :value="settingsStore.aiModel"
            @change="settingsStore.setAiModel(($event.target as HTMLInputElement).value)"
            placeholder="codellama:7b"
          />
        </div>

        <div v-if="settingsStore.aiProvider !== 'ollama'" class="settings-section">
          <div class="settings-label">
            API Key
            <span v-if="aiKeyExists" class="key-saved">saved</span>
          </div>
          <div class="api-key-row">
            <input
              class="settings-input"
              type="password"
              v-model="aiApiKey"
              :placeholder="aiKeyExists ? '••••••••  (update key)' : 'Enter API key'"
            />
            <button class="btn-ghost save-key-btn" @click="saveApiKey" :disabled="!aiApiKey.trim()">Save</button>
          </div>
        </div>

        <div class="settings-section">
          <button
            class="btn-ghost test-btn"
            :disabled="aiHealthStatus === 'checking'"
            @click="testAiConnection"
          >
            {{ aiHealthStatus === 'checking' ? 'Checking…' : 'Test connection' }}
            <span v-if="aiHealthStatus === 'ok'" class="health-ok">OK</span>
            <span v-if="aiHealthStatus === 'fail'" class="health-fail">Failed</span>
          </button>
        </div>
      </template>
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
          <tr><td class="shortcut-key">Ctrl+Shift+S</td><td>Save all scripts</td></tr>
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

/* Tab bar */
.tabs-bar {
  display: flex; align-items: center;
  background: var(--bg-sidebar); border-bottom: 1px solid var(--border);
  height: 30px; flex-shrink: 0; overflow-x: auto; overflow-y: hidden;
}
.tabs-list { display: flex; height: 100%; }
.tab {
  display: flex; align-items: center; gap: 6px;
  padding: 0 10px; font-size: 11px; cursor: pointer;
  border-right: 1px solid var(--border); white-space: nowrap;
  color: var(--text-dim); background: transparent;
  transition: background 0.1s;
}
.tab:hover { background: var(--bg-hover); }
.tab.active { background: var(--bg); color: var(--text); border-bottom: 2px solid var(--accent); }
.tab-name { max-width: 120px; overflow: hidden; text-overflow: ellipsis; }
.tab-close {
  font-size: 10px; background: transparent; color: var(--text-muted);
  border-radius: 2px; padding: 1px 3px;
  opacity: 0; transition: opacity 0.1s;
}
.tab:hover .tab-close { opacity: 1; }
.tab-close:hover { background: var(--bg-hover); color: var(--red); }

/* Workspace instances */
.workspace { display: flex; flex-direction: column; flex: 1; overflow: hidden; }
.workspace-empty {
  flex: 1; display: flex; align-items: center; justify-content: center;
  color: var(--text-muted); font-size: 12px;
}
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

/* AI settings */
.settings-divider { height: 1px; background: var(--border); margin: 16px 0; }
.ai-toggle-row { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-dim); cursor: pointer; }
.ai-toggle-row input[type="checkbox"] { accent-color: var(--accent); width: 14px; height: 14px; }
.provider-row { display: flex; gap: 6px; }
.settings-input {
  width: 100%; padding: 6px 8px; font-size: 12px;
  background: var(--bg-input); color: var(--text);
  border: 1px solid var(--border); border-radius: var(--radius);
  font-family: var(--font-mono);
}
.settings-input:focus { border-color: var(--accent); outline: none; }
.api-key-row { display: flex; gap: 6px; }
.api-key-row .settings-input { flex: 1; }
.save-key-btn { font-size: 11px; padding: 5px 10px; white-space: nowrap; }
.save-key-btn:disabled { opacity: 0.4; cursor: not-allowed; }
.key-saved { color: var(--green); font-size: 10px; font-weight: 600; margin-left: 6px; text-transform: none; letter-spacing: 0; }
.test-btn { font-size: 11px; display: flex; align-items: center; gap: 6px; }
.health-ok { color: var(--green); font-weight: 600; }
.health-fail { color: var(--red); font-weight: 600; }

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
