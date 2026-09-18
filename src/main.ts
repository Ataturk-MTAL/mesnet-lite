import { createApp } from 'vue'
import { createPinia } from 'pinia'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import App from './App.vue'
import router from './router'
import { initTheme } from './composables/useTheme'
import 'primeicons/primeicons.css'
import 'leaflet/dist/leaflet.css'

const app = createApp(App)
app.use(createPinia())
app.use(router)

// OpenVue, PrimeVue 4.5.5'in MIT lisanslı devamıdır; lisans anahtarı gerektirmez.
// darkModeSelector varsayılanı 'system'dir; kullanıcı kendi seçebilsin diye
// sınıf tabanlı seçiciye alınıyor. Sınıf adı useTheme.ts ile aynı olmalıdır.
app.use(OpenVue, {
  theme: {
    preset: Aura,
    options: { darkModeSelector: '.app-dark' },
  },
})
app.use(ToastService)
app.use(ConfirmationService)
// İkon butonlarının ne yaptığını göstermek için ipucu yönergesi.
app.directive('tooltip', Tooltip)

// Tema, ilk boyamadan önce uygulanır ki açılışta yanıp sönme olmasın.
initTheme()

app.mount('#app')
