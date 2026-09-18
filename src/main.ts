import { createApp } from 'vue'
import { createPinia } from 'pinia'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import App from './App.vue'
import router from './router'
import 'primeicons/primeicons.css'
import 'leaflet/dist/leaflet.css'

const app = createApp(App)
app.use(createPinia())
app.use(router)
// OpenVue, PrimeVue 4.5.5'in MIT lisanslı devamıdır; lisans anahtarı gerektirmez.
app.use(OpenVue, { theme: { preset: Aura } })
app.use(ToastService)
app.use(ConfirmationService)
app.mount('#app')
