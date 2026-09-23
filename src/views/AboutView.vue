<template>
  <div class="page">
    <Card class="about-card">
      <template #content>
        <img :src="logoUrl" :alt="labels.app.logoAlt" class="about-logo" />
        <h1 class="app-name">{{ labels.about.appName }}</h1>
        <p class="school">{{ labels.about.school }}</p>
        <p class="field">{{ labels.about.field }}</p>
        <p class="purpose">{{ labels.about.purpose }}</p>

        <div class="about-footer">
          <p class="license">{{ labels.about.license }}</p>
          <Button
            :label="labels.about.sourceCode"
            icon="pi pi-github"
            severity="secondary"
            outlined
            :aria-label="labels.about.sourceCode"
            @click="openSourceCode"
          />
          <p class="logo-license-note">{{ labels.about.logoLicenseNote }}</p>
        </div>
      </template>
    </Card>
  </div>
</template>

<script setup lang="ts">
// Uygulama adı, okul bilgisi, amaç cümlesi, lisans ve kaynak kod bağlantısını
// gösterir; sürüm numarası veya iletişim bilgisi YOKTUR.
import { openUrl } from '@tauri-apps/plugin-opener'
import { useToast } from 'openvue/usetoast'
import { labels } from '../i18n/labels'
import logoUrl from '../assets/ataturk-mtal.png'

const toast = useToast()

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

async function openSourceCode(): Promise<void> {
  try {
    await openUrl(labels.about.sourceCodeUrl)
  } catch (error: unknown) {
    showError(error)
  }
}
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

.about-logo {
  display: block;
  width: 8rem;
  height: 8rem;
  object-fit: contain;
  margin: 0 auto 1rem;
}

.app-name {
  font-size: 1.75rem;
  font-weight: 600;
  margin: 0 0 0.75rem;
  text-align: center;
}

.school,
.field {
  margin: 0 0 0.5rem;
  font-size: 1rem;
  color: var(--p-text-color);
  text-align: center;
}

.purpose {
  margin: 1rem 0 0;
  color: var(--p-text-muted-color);
  line-height: 1.6;
  text-align: center;
}

.about-footer {
  margin-top: 1.5rem;
  padding-top: 1rem;
  border-top: 1px solid var(--p-content-border-color);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
}

.license {
  margin: 0;
  font-size: 0.875rem;
  color: var(--p-text-muted-color);
}

.logo-license-note {
  margin: 0;
  font-size: 0.75rem;
  color: var(--p-text-muted-color);
  text-align: center;
}
</style>
