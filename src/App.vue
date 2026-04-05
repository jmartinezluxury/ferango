<script setup lang="ts">
import { ref, computed, watch, onMounted, provide } from 'vue'
import { useConnectionsStore } from './stores/connections'
import { useEditorStore } from './stores/editor'
import { useSettingsStore } from './stores/settings'
import { saveAiApiKey, getAiApiKeyExists, aiCheckHealth } from './lib/tauri'
import ConnectionTree from './components/ConnectionTree.vue'
import ScriptBrowser from './components/ScriptBrowser.vue'
import QueryEditor from './components/QueryEditor.vue'
import ResultViewer from './components/ResultViewer.vue'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Button } from './components/ui/button'
import { Switch } from './components/ui/switch'
import { Label } from './components/ui/label'
import { Input } from './components/ui/input'
import { Separator } from './components/ui/separator'
import { Badge } from './components/ui/badge'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './components/ui/select'
import { Settings, Keyboard } from 'lucide-vue-next'

const connStore = useConnectionsStore()
const editorStore = useEditorStore()
const settingsStore = useSettingsStore()

// ── Breadcrumb: reads from the active tab's stored context ───────────────────
const activeTabConnName = computed(() => {
  const tab = editorStore.tabs[editorStore.activeTabIndex]
  if (!tab?.connId) return connStore.activeConn?.name ?? ''
  const conn = connStore.connections.find(c => c.id === tab.connId)
  return conn?.name ?? connStore.activeConn?.name ?? ''
})
const activeTabDb = computed(() => {
  const tab = editorStore.tabs[editorStore.activeTabIndex]
  return tab?.dbName || connStore.activeDb || ''
})

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
  <div class="flex flex-col h-screen overflow-hidden">
    <!-- Toolbar -->
    <div class="h-[38px] bg-sidebar border-b border-border flex items-center px-3 gap-2 shrink-0 select-none" style="-webkit-app-region: drag">
      <span class="text-[13px] font-extrabold tracking-tight" style="-webkit-app-region: no-drag; filter: drop-shadow(0 0 6px rgba(247,148,29,0.4))">
        <span class="text-white">Fer</span><span class="text-primary">ango</span>
      </span>
      <div class="w-px h-4 bg-border" />
      <span class="flex items-center gap-1.5 text-[13px]" style="-webkit-app-region: no-drag">
        <span v-if="activeTabConnName" class="text-muted-foreground">{{ activeTabConnName }}</span>
        <template v-if="activeTabDb">
          <span class="text-muted-foreground/50">›</span>
          <span class="text-ferango-blue">{{ activeTabDb }}</span>
        </template>
        <span v-if="!activeTabConnName" class="text-muted-foreground italic">No connection selected</span>
      </span>
      <span class="flex-1" />
      <Button variant="ghost" size="icon" class="h-7 w-7 text-muted-foreground hover:text-foreground" style="-webkit-app-region: no-drag" title="Keyboard shortcuts" @click="shortcutsOpen = true">
        <Keyboard class="h-4 w-4" />
      </Button>
      <Button variant="ghost" size="icon" class="h-7 w-7 text-muted-foreground hover:text-foreground" style="-webkit-app-region: no-drag" title="Settings" @click="settingsOpen = true">
        <Settings class="h-4 w-4" />
      </Button>
    </div>

    <div class="flex flex-1 overflow-hidden">
      <!-- Sidebar -->
      <div class="flex flex-col bg-sidebar overflow-hidden shrink-0 min-w-[180px]" :style="{ width: sidebarW + 'px' }">
        <ConnectionTree />
        <Separator />
        <ScriptBrowser />
      </div>
      <div class="resize-h" @mousedown.prevent="startResizeH" />

      <!-- Main area -->
      <div class="flex-1 flex flex-col overflow-hidden">
        <!-- Tab bar -->
        <div v-if="editorStore.tabs.length > 0" class="flex items-center bg-sidebar border-b border-border h-[30px] shrink-0 overflow-x-auto overflow-y-hidden">
          <div class="flex h-full">
            <div
              v-for="(tab, i) in editorStore.tabs"
              :key="tab.script.path"
              :class="[
                'flex items-center min-w-[100px] pl-2.5 pr-1.5 text-xs cursor-pointer border-r border-border whitespace-nowrap transition-colors',
                i === editorStore.activeTabIndex
                  ? 'bg-background text-foreground border-b-2 border-b-primary'
                  : 'text-muted-foreground hover:bg-accent'
              ]"
              @click="editorStore.switchTab(i)"
            >
              <span class="overflow-hidden text-ellipsis flex-1">{{ tab.script.name }}{{ tab.modified ? ' ●' : '' }}</span>
              <button
                class="text-[10px] leading-none rounded ml-2 opacity-0 hover:text-destructive transition-opacity shrink-0"
                :class="{ '!opacity-100': i === editorStore.activeTabIndex }"
                title="Close"
                @click.stop="editorStore.closeTab(i)"
              >✕</button>
            </div>
          </div>
        </div>

        <!-- Single workspace: one QueryEditor (model-swapped) + per-tab ResultViewers -->
        <template v-if="editorStore.tabs.length > 0">
          <div class="shrink-0 overflow-hidden min-h-[80px]" :style="{ height: editorH + 'px' }">
            <QueryEditor />
          </div>
          <div class="resize-v" @mousedown.prevent="startResizeV" />
          <div class="flex-1 overflow-hidden">
            <div
              v-for="(tab, i) in editorStore.tabs"
              :key="tab.script.path"
              v-show="i === editorStore.activeTabIndex"
              class="h-full"
            >
              <ResultViewer :tabIndex="i" />
            </div>
          </div>
        </template>

        <!-- Empty state when no tabs open -->
        <div v-if="editorStore.tabs.length === 0" class="flex-1 flex items-center justify-center text-muted-foreground text-xs">
          <span>Open a script or select a collection to get started</span>
        </div>
      </div>
    </div>

    <!-- Status bar -->
    <div class="h-6 bg-sidebar border-t border-border flex items-center px-2.5 gap-1.5 text-[11px] text-muted-foreground shrink-0">
      <template v-if="editorStore.result">
        <span :class="editorStore.result.success ? 'text-ferango-green' : 'text-ferango-red'">
          {{ editorStore.result.success ? `${editorStore.result.rows} docs` : 'Error' }}
        </span>
        <span class="text-muted-foreground/50"> · </span>
        <span>{{ editorStore.result.elapsed_ms }}ms</span>
      </template>
      <span v-else class="text-muted-foreground/50">Ready</span>
      <span class="flex-1" />
      <span class="text-[10px] font-mono text-muted-foreground/50">{{ editorStore.scriptsDir }}</span>
    </div>
  </div>

  <!-- Settings modal (shadcn Dialog) -->
  <Dialog :open="settingsOpen" @update:open="settingsOpen = $event">
    <DialogContent class="sm:max-w-[400px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Settings</DialogTitle>
      </DialogHeader>

      <div class="space-y-5">
        <!-- Theme -->
        <div class="space-y-2">
          <Label class="text-[10px] uppercase tracking-wider text-muted-foreground">Theme</Label>
          <div class="flex gap-2">
            <Button
              v-for="t in (['dark', 'light'] as const)" :key="t"
              :variant="settingsStore.theme === t ? 'default' : 'outline'"
              size="sm" class="flex-1 text-xs capitalize"
              @click="settingsStore.setTheme(t)"
            >{{ t }}</Button>
          </div>
        </div>

        <!-- Font size -->
        <div class="space-y-2">
          <Label class="text-[10px] uppercase tracking-wider text-muted-foreground">Editor font size</Label>
          <div class="flex items-center gap-3">
            <Button variant="outline" size="icon" class="h-7 w-7 text-base" :disabled="settingsStore.fontSize <= 10" @click="settingsStore.setFontSize(settingsStore.fontSize - 1)">-</Button>
            <span class="text-sm font-mono min-w-[36px] text-center">{{ settingsStore.fontSize }}px</span>
            <Button variant="outline" size="icon" class="h-7 w-7 text-base" :disabled="settingsStore.fontSize >= 24" @click="settingsStore.setFontSize(settingsStore.fontSize + 1)">+</Button>
          </div>
        </div>

        <Separator />

        <!-- AI Autocomplete -->
        <div class="flex items-center justify-between">
          <Label class="text-[10px] uppercase tracking-wider text-muted-foreground">AI Autocomplete</Label>
          <Switch :checked="settingsStore.aiEnabled" @update:checked="settingsStore.setAiEnabled" />
        </div>

        <template v-if="settingsStore.aiEnabled">
          <!-- Provider -->
          <div class="space-y-2">
            <Label class="text-[10px] uppercase tracking-wider text-muted-foreground">Provider</Label>
            <Select :model-value="settingsStore.aiProvider" @update:model-value="settingsStore.setAiProvider($event as 'ollama' | 'openai' | 'claude')">
              <SelectTrigger class="h-8 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="ollama">Ollama</SelectItem>
                <SelectItem value="openai">OpenAI</SelectItem>
                <SelectItem value="claude">Claude</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <!-- Endpoint -->
          <div class="space-y-2">
            <Label class="text-[10px] uppercase tracking-wider text-muted-foreground">Endpoint</Label>
            <Input
              class="h-8 text-xs font-mono"
              :model-value="settingsStore.aiEndpoint"
              @change="settingsStore.setAiEndpoint(($event.target as HTMLInputElement).value)"
              placeholder="http://localhost:11434"
            />
          </div>

          <!-- Model -->
          <div class="space-y-2">
            <Label class="text-[10px] uppercase tracking-wider text-muted-foreground">Model</Label>
            <Input
              class="h-8 text-xs font-mono"
              :model-value="settingsStore.aiModel"
              @change="settingsStore.setAiModel(($event.target as HTMLInputElement).value)"
              placeholder="codellama:7b"
            />
          </div>

          <!-- API Key -->
          <div v-if="settingsStore.aiProvider !== 'ollama'" class="space-y-2">
            <Label class="text-[10px] uppercase tracking-wider text-muted-foreground">
              API Key
              <Badge v-if="aiKeyExists" variant="secondary" class="ml-2 text-[9px] text-green-500">saved</Badge>
            </Label>
            <div class="flex gap-2">
              <Input
                class="h-8 text-xs font-mono flex-1"
                type="password"
                v-model="aiApiKey"
                :placeholder="aiKeyExists ? '••••••••  (update key)' : 'Enter API key'"
              />
              <Button variant="outline" size="sm" class="text-xs shrink-0" @click="saveApiKey" :disabled="!aiApiKey.trim()">Save</Button>
            </div>
          </div>

          <!-- Test connection -->
          <Button
            variant="outline" size="sm" class="text-xs w-full"
            :disabled="aiHealthStatus === 'checking'"
            @click="testAiConnection"
          >
            {{ aiHealthStatus === 'checking' ? 'Checking...' : 'Test connection' }}
            <Badge v-if="aiHealthStatus === 'ok'" variant="secondary" class="ml-2 text-green-500 text-[9px]">OK</Badge>
            <Badge v-if="aiHealthStatus === 'fail'" variant="destructive" class="ml-2 text-[9px]">Failed</Badge>
          </Button>
        </template>
      </div>
    </DialogContent>
  </Dialog>

  <!-- Shortcuts modal (shadcn Dialog) -->
  <Dialog :open="shortcutsOpen" @update:open="shortcutsOpen = $event">
    <DialogContent class="sm:max-w-[440px] bg-card border-border">
      <DialogHeader>
        <DialogTitle class="text-sm font-semibold">Keyboard shortcuts</DialogTitle>
      </DialogHeader>

      <div class="space-y-1">
        <div v-for="s in [
          { key: 'Ctrl+Enter', action: 'Run statement at cursor' },
          { key: 'Select + Ctrl+Enter', action: 'Run selected statements' },
          { key: 'Run all button', action: 'Run all statements in file' },
          { key: 'Shift+Alt+F', action: 'Format document' },
          { key: 'Ctrl+S', action: 'Save script' },
          { key: 'Ctrl+Shift+S', action: 'Save all scripts' },
          { key: 'Ctrl+F', action: 'Find in editor' },
          { key: 'Ctrl+Z', action: 'Undo' },
          { key: 'Ctrl+Shift+Z', action: 'Redo' },
          { key: 'Ctrl+/', action: 'Toggle line comment' },
        ]" :key="s.key" class="flex items-center justify-between py-1.5 text-xs">
          <span class="text-muted-foreground">{{ s.action }}</span>
          <kbd class="font-mono text-[11px] text-primary bg-secondary px-1.5 py-0.5 rounded">{{ s.key }}</kbd>
        </div>
      </div>
    </DialogContent>
  </Dialog>

  <div class="toast-container">
    <div v-for="t in toasts" :key="t.id" :class="['toast', t.type]">{{ t.msg }}</div>
  </div>
</template>

<style scoped>
/* Only non-Tailwind styles that can't be expressed as utility classes */
</style>
