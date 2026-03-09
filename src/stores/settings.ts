import { defineStore } from 'pinia'
import { ref } from 'vue'
import { loadSettings, saveSettings } from '../lib/tauri'

export const useSettingsStore = defineStore('settings', () => {
  const theme = ref<'dark' | 'light'>('dark')
  const fontSize = ref(13)
  const lastDbs = ref<Record<string, string>>({})

  // AI settings
  const aiEnabled = ref(true)
  const aiProvider = ref<'ollama' | 'openai' | 'claude'>('ollama')
  const aiEndpoint = ref('http://localhost:11434')
  const aiModel = ref('codellama:7b')

  function applyTheme(t: string) {
    document.documentElement.setAttribute('data-theme', t)
  }

  function applyFontSize(size: number) {
    document.documentElement.style.setProperty('--font-size-ui', `${size}px`)
  }

  function currentSettings() {
    return {
      theme: theme.value,
      font_size: fontSize.value,
      last_dbs: lastDbs.value,
      ai_enabled: aiEnabled.value,
      ai_provider: aiProvider.value,
      ai_endpoint: aiEndpoint.value,
      ai_model: aiModel.value,
    }
  }

  async function init() {
    const s = await loadSettings()
    theme.value = s.theme as 'dark' | 'light'
    fontSize.value = s.font_size
    lastDbs.value = s.last_dbs ?? {}
    aiEnabled.value = s.ai_enabled ?? true
    aiProvider.value = (s.ai_provider ?? 'ollama') as 'ollama' | 'openai' | 'claude'
    aiEndpoint.value = s.ai_endpoint ?? 'http://localhost:11434'
    aiModel.value = s.ai_model ?? 'codellama:7b'
    applyTheme(s.theme)
    applyFontSize(s.font_size)
  }

  async function setTheme(t: 'dark' | 'light') {
    theme.value = t
    applyTheme(t)
    await saveSettings(currentSettings())
  }

  async function setFontSize(size: number) {
    fontSize.value = size
    applyFontSize(size)
    await saveSettings(currentSettings())
  }

  async function saveLastDb(connId: string, db: string) {
    lastDbs.value[connId] = db
    await saveSettings(currentSettings())
  }

  async function setAiEnabled(v: boolean) {
    aiEnabled.value = v
    await saveSettings(currentSettings())
  }

  async function setAiProvider(p: 'ollama' | 'openai' | 'claude') {
    aiProvider.value = p
    // Auto-fill default endpoints
    const defaults: Record<string, { endpoint: string; model: string }> = {
      ollama: { endpoint: 'http://localhost:11434', model: 'codellama:7b' },
      openai: { endpoint: 'https://api.openai.com', model: 'gpt-4o-mini' },
      claude: { endpoint: 'https://api.anthropic.com', model: 'claude-haiku-4-5-20241022' },
    }
    const d = defaults[p]
    if (d) {
      aiEndpoint.value = d.endpoint
      aiModel.value = d.model
    }
    await saveSettings(currentSettings())
  }

  async function setAiEndpoint(e: string) {
    aiEndpoint.value = e
    await saveSettings(currentSettings())
  }

  async function setAiModel(m: string) {
    aiModel.value = m
    await saveSettings(currentSettings())
  }

  return {
    theme, fontSize, lastDbs,
    aiEnabled, aiProvider, aiEndpoint, aiModel,
    init, setTheme, setFontSize, saveLastDb,
    setAiEnabled, setAiProvider, setAiEndpoint, setAiModel,
  }
})
