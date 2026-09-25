<template>
  <!-- Giriş yapılmamışken uygulama kabuğu (kenar çubuğu, üst çubuk, RouterView)
       HİÇ çizilmez; onun yerine giriş ekranı gelir. Rota koruması kurulmaz —
       oturum yalnız bellekte olduğu için sayfa yenilemesinde zaten sıfırlanır. -->
  <AppSidebar v-if="isAuthenticated" />
  <LoginView v-else />
  <Toast />
  <ConfirmDialog />
</template>

<script setup lang="ts">
import { watch } from 'vue'
import AppSidebar from './components/layout/AppSidebar.vue'
import LoginView from './views/LoginView.vue'
import { isAuthenticated } from './composables/useAuth'
import { useSelectionStore } from './stores/selection'

const selection = useSelectionStore()

// Oturum kapanınca ekran seçimleri (öğretmen seçimi, tarihçe filtreleri,
// aramalar) sıfırlanır; başka bir kullanıcı öncekinin filtrelerini devralmaz.
watch(isAuthenticated, (value) => {
  if (!value) selection.reset()
})
</script>
