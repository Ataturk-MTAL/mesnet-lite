<template>
  <div class="page">
    <Card class="about-card">
      <template #content>
        <h1 class="app-name">{{ labels.about.appName }}</h1>
        <p class="school">{{ labels.about.school }}</p>
        <p class="field">{{ labels.about.field }}</p>
        <p class="purpose">{{ labels.about.purpose }}</p>
      </template>
    </Card>
  </div>
</template>

<script setup lang="ts">
// Yalnızca uygulama adı, okul bilgisi ve amaç cümlesi gösterir; süs, sürüm
// numarası veya iletişim bilgisi YOKTUR — brief bunları açıkça yasaklıyor.
import { labels } from '../i18n/labels'
</script>

<style scoped>
.page {
  padding: 1.5rem;
  display: flex;
  justify-content: center;
  align-items: center;
}

.about-card {
  max-width: 40rem;
  width: 100%;
}

.app-name {
  font-size: 1.75rem;
  font-weight: 600;
  margin: 0 0 0.75rem;
}

.school,
.field {
  margin: 0 0 0.5rem;
  font-size: 1rem;
  color: var(--p-text-color);
}

.purpose {
  margin: 1rem 0 0;
  color: var(--p-text-muted-color);
  line-height: 1.6;
}
</style>
