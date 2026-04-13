import { createApp } from 'vue'
import { createPinia } from 'pinia'
import Toast from 'vue-toastification'
import 'vue-toastification/dist/index.css'
import App from './App.vue'
import { router } from './router'
import './style.css'

const app = createApp(App)
app.use(createPinia())
app.use(router)
app.use(Toast, {
  position: 'top-right' as const,
  timeout: 4000,
  closeOnClick: true,
  pauseOnHover: true,
})
app.mount('#app')
