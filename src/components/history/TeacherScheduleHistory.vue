<template>
  <Card class="schedule-history" data-testid="teacher-schedule-history">
    <template #title>{{ labels.availability.historyTitle }}</template>
    <template #content>
      <p v-if="sortedEntries.length === 0 && !isLoading" class="schedule-history-empty" data-testid="schedule-history-empty">
        {{ labels.availability.historyEmpty }}
      </p>

      <ul v-else class="schedule-history-list" :aria-busy="isLoading">
        <li
          v-for="entry in sortedEntries"
          :key="entry.changeSetId"
          class="schedule-history-item"
          :class="{
            'schedule-history-item--active': entry.changeSetId === activeEntryId,
            'schedule-history-item--muted': isMuted(entry),
          }"
          :data-testid="`schedule-history-item-${entry.changeSetId}`"
        >
          <div class="schedule-history-badges">
            <Tag
              v-if="entry.changeSetId === activeEntryId"
              :value="labels.availability.historyActiveBadge"
              severity="success"
              data-testid="schedule-history-active-badge"
            />
            <Tag
              v-if="entry.revokedByChangeSetId !== null"
              :value="labels.changeHistory.revokedBadge"
              severity="secondary"
              data-testid="schedule-history-revoked-badge"
            />
            <Tag
              v-if="isRevocationRecord(entry)"
              :value="labels.availability.historyRevocationBadge"
              severity="secondary"
              data-testid="schedule-history-revocation-badge"
            />
            <Tag v-if="entry.kind === 'correct'" :value="labels.availability.historyCorrectionBadge" severity="info" />
          </div>

          <div class="schedule-history-body">
            <div class="schedule-history-effective">
              {{ labels.availability.historyEffectiveDate }}: {{ entry.effectiveDate }}
            </div>
            <div class="schedule-history-reason">{{ entry.reason }}</div>
            <div class="schedule-history-meta">
              {{ labels.changeHistory.actor }}: {{ entry.actor }} · {{ labels.changeHistory.recordedAt }}:
              {{ entry.recordedAt }}
            </div>
          </div>

          <template v-if="entry.changeSetId === activeEntryId">
            <div class="schedule-history-actions">
              <Button
                :label="labels.availability.historyEdit"
                :aria-label="labels.availability.historyEdit"
                icon="pi pi-pencil"
                size="small"
                severity="secondary"
                outlined
                :disabled="!entry.isRevocable"
                data-testid="schedule-history-edit-button"
                @click="emit('edit', entry)"
              />
              <Button
                :label="labels.availability.historyDelete"
                :aria-label="labels.availability.historyDelete"
                icon="pi pi-trash"
                size="small"
                severity="danger"
                outlined
                :disabled="!entry.isRevocable"
                data-testid="schedule-history-delete-button"
                @click="openDeleteDialog(entry)"
              />
            </div>
            <small v-if="!entry.isRevocable" class="schedule-history-hint" data-testid="schedule-history-not-revocable">
              {{ labels.availability.historyNotRevocableHint }}
            </small>
          </template>
        </li>
      </ul>

      <Button
        v-if="nextBeforeChangeSetId !== null"
        :label="labels.changeHistory.loadMore"
        :aria-label="labels.changeHistory.loadMore"
        severity="secondary"
        outlined
        size="small"
        :loading="isLoading"
        class="schedule-history-more"
        data-testid="schedule-history-load-more"
        @click="loadMore"
      />
    </template>
  </Card>

  <Dialog
    v-model:visible="isDeleteDialogOpen"
    modal
    :header="labels.availability.historyDeleteConfirm"
    :style="{ width: '28rem' }"
    data-testid="schedule-delete-dialog"
  >
    <div class="delete-reason-field">
      <label :for="deleteReasonId">{{ labels.history.reason }}</label>
      <Textarea
        :id="deleteReasonId"
        v-model="deleteReason"
        autoResize
        rows="3"
        :placeholder="labels.history.reasonPlaceholder"
        :aria-label="labels.history.reason"
        data-testid="schedule-delete-reason-input"
      />
      <small v-if="!isDeleteReasonValid" class="field-error">{{ labels.history.reasonRequired }}</small>
    </div>
    <template #footer>
      <Button :label="labels.common.cancel" severity="secondary" outlined @click="closeDeleteDialog" />
      <Button
        :label="labels.availability.historyDelete"
        severity="danger"
        :disabled="!isDeleteReasonValid"
        data-testid="schedule-delete-submit-button"
        @click="submitDelete"
      />
    </template>
  </Dialog>

  <ImpactDialog :change="revokeChange" />
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { listHistory } from '../../api/history'
import { labels } from '../../i18n/labels'
import { useChange } from '../../composables/useChange'
import ImpactDialog from './ImpactDialog.vue'
import type { HistoryChangeSetEntry, HistoryFilter, Stream } from '../../types/models'

const props = defineProps<{
  /** Geçmişi gösterilecek öğretmen; seçili değilse liste boş kalır. */
  teacherId: number | null
  /** Aktif dönem; hem listeleme hem geri alma isteğinde kullanılır. */
  term: string
  /** Üst bileşen her başarılı kayıttan sonra bunu artırır; liste yeniden yüklenir. */
  refreshToken: number
}>()

const emit = defineEmits<{
  /** "Düzenle"ye basıldı; ızgara düzeltme moduna geçirilir. */
  edit: [entry: HistoryChangeSetEntry]
  /** "Geçmişten sil" kaydedildi; üst bileşen ızgarayı yeniler. */
  changed: []
}>()

const SCHEDULE_STREAM: Stream = 'teacher_schedule'
const PAGE_SIZE = 50

const toast = useToast()
const entries = ref<HistoryChangeSetEntry[]>([])
const nextBeforeChangeSetId = ref<number | null>(null)
const isLoading = ref(false)

/** Eski bir yanıtın yeni öğretmenin listesini ezmemesi için istek sayacı. */
let requestSeq = 0

/** En yeni üstte; gelen dizi yerinde değiştirilmez. */
const sortedEntries = computed(() => [...entries.value].sort((a, b) => b.changeSetId - a.changeSetId))

/** Geri alma kaydı: başka bir kümeyi geri alan, kendisi yeni bir program getirmeyen küme. */
function isRevocationRecord(entry: HistoryChangeSetEntry): boolean {
  return entry.revokesChangeSetId !== null && entry.kind !== 'correct'
}

function isMuted(entry: HistoryChangeSetEntry): boolean {
  return entry.revokedByChangeSetId !== null || isRevocationRecord(entry)
}

/** En son geçerli (geri alınmamış, geri alma kaydı olmayan) değişiklik. */
const activeEntryId = computed<number | null>(
  () => sortedEntries.value.find((entry) => !isMuted(entry))?.changeSetId ?? null,
)

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

function buildFilter(teacherId: number, beforeChangeSetId: number | null): HistoryFilter {
  return {
    term: props.term,
    stream: SCHEDULE_STREAM,
    subjectId: teacherId,
    companyId: null,
    teacherId: null,
    includeOpening: false,
    beforeChangeSetId,
    limit: PAGE_SIZE,
  }
}

async function loadPage(beforeChangeSetId: number | null): Promise<void> {
  const teacherId = props.teacherId
  const seq = ++requestSeq
  if (teacherId === null) {
    entries.value = []
    nextBeforeChangeSetId.value = null
    isLoading.value = false
    return
  }
  isLoading.value = true
  try {
    const response = await listHistory(buildFilter(teacherId, beforeChangeSetId))
    if (seq !== requestSeq) return
    entries.value = beforeChangeSetId === null ? response.entries : [...entries.value, ...response.entries]
    nextBeforeChangeSetId.value = response.nextBeforeChangeSetId
  } catch (error: unknown) {
    if (seq === requestSeq) showError(error)
  } finally {
    if (seq === requestSeq) isLoading.value = false
  }
}

async function loadMore(): Promise<void> {
  if (nextBeforeChangeSetId.value === null) return
  await loadPage(nextBeforeChangeSetId.value)
}

// Öğretmen, dönem ya da yenileme sayacı değişince ilk sayfa yeniden yüklenir.
watch(
  () => [props.teacherId, props.term, props.refreshToken] as const,
  () => {
    void loadPage(null)
  },
  { immediate: true },
)

// --- Geçmişten sil (revoke) ---

const revokeChange = useChange()
const deleteTarget = ref<HistoryChangeSetEntry | null>(null)
const isDeleteDialogOpen = ref(false)
const deleteReason = ref('')
const deleteReasonId = useId()
const isDeleteReasonValid = computed(() => deleteReason.value.trim().length > 0)

function openDeleteDialog(entry: HistoryChangeSetEntry): void {
  deleteTarget.value = entry
  deleteReason.value = ''
  isDeleteDialogOpen.value = true
}

function closeDeleteDialog(): void {
  isDeleteDialogOpen.value = false
}

async function submitDelete(): Promise<void> {
  const target = deleteTarget.value
  if (!target || !isDeleteReasonValid.value) return
  isDeleteDialogOpen.value = false
  await revokeChange.preview({
    term: props.term,
    effectiveDate: null,
    documentDate: null,
    reason: deleteReason.value.trim(),
    command: { type: 'revoke', changeSetId: target.changeSetId },
  })
}

// Etki penceresi, kullanıcı kapatana kadar sonucu gösterir; liste ve ızgara hemen tazelenir.
watch(
  () => revokeChange.status.value,
  (status) => {
    if (status !== 'committed') return
    emit('changed')
    void loadPage(null)
  },
)
</script>

<style scoped>
.schedule-history-empty { margin: 0; color: var(--p-text-muted-color); }
.schedule-history-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.75rem; }
.schedule-history-item {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
  border: 1px solid var(--p-content-border-color);
  border-radius: 6px;
  padding: 0.625rem 0.75rem;
}
.schedule-history-item--active { border-color: var(--p-primary-color); }
.schedule-history-badges { display: flex; flex-wrap: wrap; gap: 0.375rem; }
.schedule-history-badges:empty { display: none; }
.schedule-history-body { display: flex; flex-direction: column; gap: 0.25rem; transition: opacity 0.15s ease; }
.schedule-history-item--muted .schedule-history-body { text-decoration: line-through; opacity: 0.6; }
@media (prefers-reduced-motion: reduce) {
  .schedule-history-body { transition: none; }
}
.schedule-history-effective { font-weight: 600; font-size: 0.9375rem; }
.schedule-history-reason { font-size: 0.875rem; }
.schedule-history-meta { color: var(--p-text-muted-color); font-size: 0.75rem; }
.schedule-history-actions { display: flex; flex-wrap: wrap; gap: 0.5rem; }
.schedule-history-hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
.schedule-history-more { margin-top: 0.75rem; }
.delete-reason-field { display: flex; flex-direction: column; gap: 0.375rem; }
.delete-reason-field label { font-size: 0.875rem; font-weight: 500; }
.field-error { color: var(--p-red-500); font-size: 0.75rem; }
</style>
