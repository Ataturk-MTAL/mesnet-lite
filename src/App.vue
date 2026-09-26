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
import { storeToRefs } from 'pinia'
import AppSidebar from './components/layout/AppSidebar.vue'
import LoginView from './views/LoginView.vue'
import { useAuthStore } from './stores/auth'

// Oturum kapanınca ekran seçimleri (öğretmen seçimi, tarihçe filtreleri,
// aramalar) `stores/auth.ts`ın `signOut` eyleminde sıfırlanır; başka bir
// kullanıcı öncekinin filtrelerini devralmaz.
const { isAuthenticated } = storeToRefs(useAuthStore())
</script>
