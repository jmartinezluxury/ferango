import { defineStore } from 'pinia'
import { ref } from 'vue'
import { loadSettings, saveSettings } from '../lib/tauri'

export const useSettingsStore = defineStore('settings', () => {
  const theme = ref<'dark' | 'light'>('dark')
  const fontSize = ref(13)
  const lastDbs = ref<Record<string, string>>({})

  function applyTheme(t: string) {
    document.documentElement.setAttribute('data-theme', t)
  }

  function applyFontSize(size: number) {
    document.documentElement.style.setProperty('--font-size-ui', `${size}px`)
  }

  function currentSettings() {
    return { theme: theme.value, font_size: fontSize.value, last_dbs: lastDbs.value }
  }

  async function init() {
    const s = await loadSettings()
    theme.value = s.theme as 'dark' | 'light'
    fontSize.value = s.font_size
    lastDbs.value = s.last_dbs ?? {}
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

  return { theme, fontSize, lastDbs, init, setTheme, setFontSize, saveLastDb }
})
