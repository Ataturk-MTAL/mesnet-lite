<template>
  <Dialog
    :visible="dialogVisible"
    modal
    :header="pending ? labels.update.title(pending.version) : ''"
    :closable="!installing"
    :close-on-escape="!installing"
    :style="{ width: '32rem', maxWidth: '95vw' }"
    @update:visible="onVisibleChange"
  >
    <div v-if="pending" class="update-body">
      <p class="current-version">{{ labels.update.currentVersion(pending.currentVersion) }}</p>

      <h2 class="notes-heading">{{ labels.update.notesHeading }}</h2>
      <!-- Sürüm notları düz metin olarak basılır (HTML değil): XSS'e kapalı. -->
      <pre
        class="release-notes"
        tabindex="0"
        role="region"
        :aria-label="labels.update.notesAria"
        data-testid="update-notes"
        >{{ pending.body || labels.update.noNotes }}</pre
      >

      <Message severity="info" :closable="false" class="backup-info">
        {{ labels.update.backupInfo }}
      </Message>

      <div v-if="installing" class="progress" data-testid="update-progress">
        <span class="progress-label">{{ labels.update.downloading }}</span>
        <ProgressBar
          v-if="progressPercent !== null"
          :value="progressPercent"
          :aria-label="labels.update.progressAria"
        />
        <ProgressBar
          v-else
          mode="indeterminate"
          class="progress-indeterminate"
          :aria-label="labels.update.progressAria"
        />
      </div>
    </div>

    <template #footer>
      <Button
        :label="labels.update.later"
        :aria-label="labels.update.later"
        severity="secondary"
        outlined
        :disabled="installing"
        data-testid="update-later-button"
        @click="dismiss"
      />
      <Button
        :label="labels.update.updateNow"
        :aria-label="labels.update.updateNow"
        icon="pi pi-download"
        :loading="installing"
        :disabled="installing"
        data-testid="update-now-button"
        @click="installNow"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
// Yeni sürüm bulunduğunda sorar; "Şimdi Güncelle" indirir, kurar, yeniden başlatır.
// Kök bileşende bir kez bağlanır; durum `useUpdaterStore`'dadır.
import { labels } from '../../i18n/labels'
import { useUpdater } from '../../composables/useUpdater'

const { pending, dialogVisible, installing, progressPercent, installNow, dismiss } = useUpdater()

/** X düğmesi ve Esc: "Sonra" ile aynı (kurulum sürerken zaten kapalı). */
function onVisibleChange(visible: boolean): void {
  if (!visible && !installing.value) dismiss()
}
</script>

<style scoped>
.update-body {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.current-version {
  margin: 0;
  color: var(--p-text-muted-color);
}

.notes-heading {
  margin: 0;
  font-size: 1rem;
  font-weight: 600;
}

.release-notes {
  margin: 0;
  max-height: 12rem;
  overflow: auto;
  padding: 0.75rem;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-family: inherit;
  font-size: 0.875rem;
  line-height: 1.5;
  background: var(--p-content-hover-background);
  border: 1px solid var(--p-content-border-color);
  border-radius: var(--p-content-border-radius);
}

.progress {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.progress-label {
  font-size: 0.875rem;
  color: var(--p-text-muted-color);
}

.progress-indeterminate {
  height: 6px;
}
</style>
