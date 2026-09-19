<template>
  <Card class="history-entry" :class="{ 'history-entry--revoked': isRevoked }" data-testid="history-entry-card">
    <template #title>
      <div class="history-entry-title">
        <span class="history-entry-meta">{{ labels.changeHistory.recordedAt }}: {{ entry.recordedAt }}</span>
        <span class="history-entry-meta">{{ entry.effectiveDate }}</span>
        <span v-if="entry.documentDate" class="history-entry-meta">{{ entry.documentDate }}</span>
        <span class="history-entry-meta">{{ labels.changeHistory.actor }}: {{ entry.actor }}</span>
        <Tag v-if="isRevoked" :value="labels.changeHistory.revokedBadge" severity="secondary" data-testid="revoked-badge" />
        <Tag
          v-if="entry.warnings.length > 0"
          :value="`${labels.changeHistory.warningsBadge}: ${entry.warnings.length}`"
          severity="warn"
        />
      </div>
    </template>
    <template #subtitle>{{ entry.reason }}</template>
    <template #content>
      <ul class="history-event-list">
        <li
          v-for="row in eventRows"
          :key="row.event.eventId"
          class="history-event-row"
          :class="{ 'history-event-row--revoked': row.event.isRevoked }"
          :style="{ marginLeft: `${row.depth * 1.25}rem` }"
          :data-testid="`history-event-${row.event.eventId}`"
          :data-depth="row.depth"
        >
          <span v-if="row.depth > 0" class="history-event-caused-tag">{{ labels.changeHistory.causedBy }}</span>
          <span class="history-event-subject">{{ row.event.subjectLabel }}</span>
          <span class="history-event-kind">{{ labels.history.eventKind[row.event.kind] }}</span>
          <span class="history-event-change">{{ row.event.before ?? '—' }} → {{ row.event.after ?? '—' }}</span>
          <span class="history-event-date">{{ row.event.effectiveDate }}</span>
        </li>
      </ul>
    </template>
    <template #footer>
      <Button
        v-if="entry.isRevocable"
        :label="labels.changeHistory.revoke"
        severity="danger"
        outlined
        size="small"
        :aria-label="labels.changeHistory.revoke"
        data-testid="revoke-button"
        @click="openRevokeDialog"
      />
    </template>
  </Card>

  <Dialog
    v-model:visible="isRevokeDialogOpen"
    modal
    :header="labels.changeHistory.revokeConfirm"
    :style="{ width: '28rem' }"
    data-testid="revoke-reason-dialog"
  >
    <div class="revoke-reason-field">
      <label :for="revokeReasonId">{{ labels.history.reason }}</label>
      <Textarea
        :id="revokeReasonId"
        v-model="revokeReason"
        autoResize
        rows="3"
        :placeholder="labels.history.reasonPlaceholder"
        :aria-label="labels.history.reason"
        data-testid="revoke-reason-input"
      />
      <small v-if="!isReasonValid" class="field-error">{{ labels.history.reasonRequired }}</small>
    </div>
    <template #footer>
      <Button :label="labels.common.cancel" severity="secondary" outlined @click="closeRevokeDialog" />
      <Button
        :label="labels.changeHistory.revoke"
        severity="danger"
        :disabled="!isReasonValid"
        data-testid="revoke-submit-button"
        @click="submitRevoke"
      />
    </template>
  </Dialog>

  <ImpactDialog :change="change" />
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { labels } from '../../i18n/labels'
import { useChange } from '../../composables/useChange'
import ImpactDialog from './ImpactDialog.vue'
import type { HistoryChangeSetEntry, HistoryEventEntry } from '../../types/models'

const props = defineProps<{
  /** Tek bir değişiklik kümesi; olayları `causedByEventId` ile iç içe gösterilir. */
  entry: HistoryChangeSetEntry
  /** Geri alma isteğinin gönderileceği aktif dönem. */
  term: string
}>()

const emit = defineEmits<{ revoked: [] }>()

const isRevoked = computed(() => props.entry.revokedByChangeSetId !== null)

/** Tek bir tarihçe satırı; girinti derinliği `causedByEventId` zincirinden türetilir. */
interface EventRow {
  event: HistoryEventEntry
  depth: number
}

/** Olayları neden-sonuç zincirine göre girintili düz bir listeye çevirir (DFS). */
function buildEventRows(events: readonly HistoryEventEntry[]): EventRow[] {
  const childrenByCause = new Map<number | null, HistoryEventEntry[]>()
  for (const event of events) {
    const bucket = childrenByCause.get(event.causedByEventId) ?? []
    childrenByCause.set(event.causedByEventId, [...bucket, event])
  }

  const rows: EventRow[] = []
  const visited = new Set<number>()
  const visit = (causeId: number | null, depth: number): void => {
    for (const event of childrenByCause.get(causeId) ?? []) {
      rows.push({ event, depth })
      visited.add(event.eventId)
      visit(event.eventId, depth + 1)
    }
  }
  visit(null, 0)

  // Nedeni bu kümede bulunmayan (beklenmeyen) olaylar veri kaybetmesin diye en üstte gösterilir.
  for (const event of events) {
    if (!visited.has(event.eventId)) rows.push({ event, depth: 0 })
  }
  return rows
}

const eventRows = computed(() => buildEventRows(props.entry.events))

const change = useChange()
const isRevokeDialogOpen = ref(false)
const revokeReason = ref('')
const revokeReasonId = useId()
const isReasonValid = computed(() => revokeReason.value.trim().length > 0)

function openRevokeDialog(): void {
  revokeReason.value = ''
  isRevokeDialogOpen.value = true
}

function closeRevokeDialog(): void {
  isRevokeDialogOpen.value = false
}

async function submitRevoke(): Promise<void> {
  if (!isReasonValid.value) return
  isRevokeDialogOpen.value = false
  await change.preview({
    term: props.term,
    effectiveDate: null,
    documentDate: null,
    reason: revokeReason.value,
    command: { type: 'revoke', changeSetId: props.entry.changeSetId },
  })
}

// Geri alma kaydedilince üst bileşen listeyi yeniler; pencere kullanıcı
// "Vazgeç"e basana kadar açık kalır ki sonucu okuyabilsin.
watch(
  () => change.status.value,
  (status) => {
    if (status === 'committed') emit('revoked')
  },
)
</script>

<style scoped>
.history-entry :deep(.p-card-body) { transition: opacity 0.15s ease; }
.history-entry--revoked :deep(.p-card-body) { text-decoration: line-through; opacity: 0.6; }
@media (prefers-reduced-motion: reduce) {
  .history-entry :deep(.p-card-body) { transition: none; }
}
.history-entry-title { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; font-size: 0.9375rem; font-weight: 500; }
.history-entry-meta { color: var(--p-text-muted-color); font-size: 0.8125rem; font-weight: 400; }
.history-event-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.5rem; }
.history-event-row {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.5rem 0.75rem;
  font-size: 0.875rem;
  border-left: 2px solid var(--p-content-border-color);
  padding-left: 0.75rem;
}
.history-event-row--revoked { text-decoration: line-through; opacity: 0.6; }
.history-event-caused-tag { color: var(--p-text-muted-color); font-size: 0.75rem; font-style: italic; }
.history-event-subject { font-weight: 600; }
.history-event-kind { color: var(--p-text-muted-color); }
.history-event-date { margin-left: auto; color: var(--p-text-muted-color); font-size: 0.8125rem; }
.revoke-reason-field { display: flex; flex-direction: column; gap: 0.375rem; }
.revoke-reason-field label { font-size: 0.875rem; font-weight: 500; }
.field-error { color: var(--p-red-500); font-size: 0.75rem; }
</style>
