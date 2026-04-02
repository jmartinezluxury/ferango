import './lib/monaco-workers' // must be first
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import '@fontsource/roboto/400.css'
import '@fontsource/roboto/500.css'
import '@fontsource/roboto/700.css'
import './style.css'

const app = createApp(App)
app.use(createPinia())
app.mount('#app')
