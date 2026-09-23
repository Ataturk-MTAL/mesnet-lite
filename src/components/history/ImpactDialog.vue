<template>
  <Dialog
    :visible="visible"
    @update:visible="onVisibleChange"
    modal
    :closable="status !== 'committing'"
    :close-on-escape="status !== 'committing'"
    :header="labels.impact.title"
    :style="{ width: '38rem' }"
  >
    <div class="impact-body">
      <Tag v-if="impact?.isPlanning" :value="labels.impact.planningBadge" severity="info" />

      <Message v-if="status === 'rejected' && rejection" severity="error" :closable="false">
        <div class="impact-rejection">
          <strong>{{ labels.history.rejectionCode[rejection.code] }}</strong>
          <p>{{ rejection.reason }}</p>
          <Button
            v-if="canUseSuggestedDate"
            :label="labels.history.useSuggestedDate"
            size="small"
            severity="secondary"
            outlined
            data-testid="impact-suggested-date-button"
            @click="props.change.useSuggestedDate()"
          />
        </div>
      </Message>

      <Message v-if="status === 'error'" severity="error" :closable="false">{{ errorMessage }}</Message>

      <div v-if="status === 'previewing' || status === 'committing'" class="impact-progress">
        <ProgressBar mode="indeterminate" style="height: 6px" />
        <p class="impact-status-note">
          {{ status === 'previewing' ? labels.impact.previewing : labels.impact.committing }}
        </p>
      </div>

      <Message v-if="status === 'committed'" severity="success" :closable="false">{{ labels.impact.committed }}</Message>

      <section class="impact-section">
        <h3>{{ labels.impact.primarySection }}</h3>
        <ul v-if="primaryLines.length > 0" class="impact-line-list">
          <li v-for="(line, index) in primaryLines" :key="`primary-${index}`" class="impact-line">
            <ImpactLineRow :line="line" />
          </li>
        </ul>
        <p v-else class="impact-empty-note">{{ labels.impact.noPrimary }}</p>
      </section>

      <section class="impact-section">
        <h3>{{ labels.impact.automaticSection }}</h3>
        <ul v-if="automaticLines.length > 0" class="impact-line-list">
          <li v-for="(line, index) in automaticLines" :key="`automatic-${index}`" class="impact-line">
            <ImpactLineRow :line="line" />
          </li>
        </ul>
        <p v-else class="impact-empty-note">{{ labels.impact.noAutomatic }}</p>
      </section>

      <section class="impact-section">
        <h3>{{ labels.impact.warningsSection }}</h3>
        <ul v-if="warnings.length > 0" class="impact-line-list">
          <li v-for="(warning, index) in warnings" :key="`warning-${index}`" class="impact-line">
            <strong>{{ labels.history.warningCode[warning.code] }}</strong>
            <p>{{ warning.message }}</p>
            <p class="impact-meta">
              {{ warning.subjectLabel }} · {{ warning.fromDate }}<template v-if="warning.toDate"> – {{ warning.toDate }}</template>
            </p>
          </li>
        </ul>
        <p v-else class="impact-empty-note">{{ labels.impact.noWarnings }}</p>
      </section>

      <section class="impact-section">
        <h3>{{ labels.impact.noticesSection }}</h3>
        <ul v-if="notices.length > 0" class="impact-line-list">
          <li v-for="(notice, index) in notices" :key="`notice-${index}`" class="impact-line">
            <strong>{{ labels.history.noticeCode[notice.code] }}</strong>
            <p>{{ notice.message }}</p>
            <p class="impact-meta">{{ notice.subjectLabel }} · {{ notice.date }}</p>
          </li>
        </ul>
        <p v-else class="impact-empty-note">{{ labels.impact.noNotices }}</p>
      </section>

      <Message
        v-if="impact?.shadowedUntil"
        severity="warn"
        :closable="false"
        data-testid="impact-shadowed-warning"
      >
        {{ labels.impact.shadowedNote(impact.shadowedUntil) }}
      </Message>
    </div>

    <template #footer>
      <Button
        :label="labels.impact.cancel"
        severity="secondary"
        outlined
        :disabled="status === 'committing'"
        data-testid="impact-cancel-button"
        @click="onCancel"
      />
      <Button
        :label="labels.impact.confirm"
        :disabled="status !== 'previewed'"
        :loading="status === 'committing'"
        data-testid="impact-confirm-button"
        @click="onConfirm"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { labels } from '../../i18n/labels'
import type { useChange } from '../../composables/useChange'
import ImpactLineRow from './ImpactLineRow.vue'

const props = defineProps<{
  /** `useChange()` çağrısının tam dönüş değeri; durum ve eylemler (confirm/useSuggestedDate/reset) burada bir arada. */
  change: ReturnType<typeof useChange>
}>()

const emit = defineEmits<{ confirm: []; cancel: [] }>()

const status = computed(() => props.change.status.value)
const impact = computed(() => props.change.impact.value)
const rejection = computed(() => props.change.rejection.value)
const canUseSuggestedDate = computed(() => props.change.canUseSuggestedDate.value)
const errorMessage = computed(() => props.change.errorMessage.value)

const primaryLines = computed(() => impact.value?.primary ?? [])
const automaticLines = computed(() => impact.value?.automatic ?? [])
const warnings = computed(() => impact.value?.warnings ?? [])
const notices = computed(() => impact.value?.notices ?? [])

/** Pencere yalnız bir değişiklik önizlenirken/kaydedilirken görünür; kapanış `reset()` ile durur. */
const visible = computed(() => status.value !== 'idle')

function onVisibleChange(next: boolean): void {
  if (!next) onCancel()
}

function onCancel(): void {
  if (status.value === 'committing') return
  props.change.reset()
  emit('cancel')
}

async function onConfirm(): Promise<void> {
  if (status.value !== 'previewed') return
  emit('confirm')
  await props.change.confirm()
}
</script>

<style scoped>
.impact-body { display: flex; flex-direction: column; gap: 1rem; }
.impact-rejection { display: flex; flex-direction: column; gap: 0.5rem; align-items: flex-start; }
.impact-progress { display: flex; flex-direction: column; gap: 0.375rem; }
.impact-status-note { color: var(--p-text-muted-color); font-size: 0.875rem; margin: 0; }
.impact-section h3 { margin: 0 0 0.5rem; font-size: 0.9375rem; }
.impact-line-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.5rem; }
.impact-line { border: 1px solid var(--p-content-border-color); border-radius: 6px; padding: 0.5rem 0.75rem; }
.impact-line p { margin: 0.25rem 0 0; }
.impact-meta { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.impact-empty-note { color: var(--p-text-muted-color); font-size: 0.875rem; margin: 0; }
</style>
