<template>
  <div class="page">
    <div class="page-header">
      <h1 class="page-title">{{ labels.changeHistory.title }}</h1>
    </div>

    <Message severity="secondary" :closable="false">{{ labels.history.subtitle }}</Message>

    <div class="history-filters">
      <div class="filter-field">
        <label for="history-filter-stream">{{ labels.changeHistory.filterStream }}</label>
        <Select
          inputId="history-filter-stream"
          v-model="filterStream"
          :options="streamOptions"
          optionLabel="label"
          optionValue="value"
          showClear
          :placeholder="labels.changeHistory.filterStream"
          :aria-label="labels.changeHistory.filterStream"
        />
      </div>

      <div class="filter-field">
        <label for="history-filter-subject">{{ labels.changeHistory.filterSubject }}</label>
        <InputNumber
          inputId="history-filter-subject"
          v-model="filterSubjectId"
          :min="1"
          :useGrouping="false"
          showButtons
          :aria-label="labels.changeHistory.filterSubject"
        />
      </div>

      <div class="filter-field">
        <label for="history-filter-company">{{ labels.changeHistory.filterCompany }}</label>
        <Select
          inputId="history-filter-company"
          v-model="filterCompanyId"
          :options="companies"
          optionLabel="name"
          optionValue="id"
          filter
          showClear
          :placeholder="labels.changeHistory.filterCompany"
          :aria-label="labels.changeHistory.filterCompany"
        />
      </div>

      <div class="filter-field">
        <label for="history-filter-teacher">{{ labels.changeHistory.filterTeacher }}</label>
        <Select
          inputId="history-filter-teacher"
          v-model="filterTeacherId"
          :options="teacherOptions"
          optionLabel="label"
          optionValue="id"
          filter
          showClear
          :placeholder="labels.changeHistory.filterTeacher"
          :aria-label="labels.changeHistory.filterTeacher"
        />
      </div>

      <div class="filter-field filter-field--inline">
        <Checkbox inputId="history-filter-opening" v-model="includeOpening" :binary="true" />
        <label for="history-filter-opening">{{ labels.changeHistory.includeOpening }}</label>
      </div>
    </div>

    <p v-if="isLoading && entries.length === 0">{{ labels.common.loading }}</p>
    <p v-else-if="entries.length === 0" class="history-empty">{{ labels.changeHistory.empty }}</p>

    <div v-else class="history-list">
      <HistoryEntryCard
        v-for="entry in entries"
        :key="entry.changeSetId"
        :entry="entry"
        :term="activeTerm"
        @revoked="load"
        @deleted="load"
      />
    </div>

    <Button
      v-if="nextBeforeChangeSetId !== null"
      :label="labels.changeHistory.loadMore"
      severity="secondary"
      outlined
      :loading="isLoadingMore"
      data-testid="history-load-more"
      @click="loadMore"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useToast } from 'openvue/usetoast'
import HistoryEntryCard from '../components/history/HistoryEntryCard.vue'
import { listHistory } from '../api/history'
import { companiesApi } from '../api/companies'
import { teachersApi } from '../api/teachers'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'
import { useSelectionStore } from '../stores/selection'
import type { Company, HistoryChangeSetEntry, HistoryFilter, Stream, Teacher } from '../types/models'

/** Bir sayfada istenen azami değişiklik kümesi sayısı; "Daha Fazla Yükle" bunu tekrarlar. */
const HISTORY_PAGE_SIZE = 20

// Select seçeneklerinin sırası `labels.history.stream` ile birebir eşleşir.
const STREAMS: readonly Stream[] = ['placement', 'company_hours', 'coordination', 'teacher_load', 'teacher_schedule']

const toast = useToast()
const selection = useSelectionStore()
const {
  historyStream: filterStream,
  historySubjectId: filterSubjectId,
  historyCompanyId: filterCompanyId,
  historyTeacherId: filterTeacherId,
  historyIncludeOpening: includeOpening,
} = storeToRefs(selection)

const entries = ref<HistoryChangeSetEntry[]>([])
const nextBeforeChangeSetId = ref<number | null>(null)
const isLoading = ref(false)
const isLoadingMore = ref(false)

const companies = ref<Company[]>([])
const teachers = ref<Teacher[]>([])

const streamOptions = computed(() =>
  STREAMS.map((stream) => ({ value: stream, label: labels.history.stream[stream] })),
)

const teacherOptions = computed(() =>
  teachers.value.map((teacher) => ({ id: teacher.id, label: `${teacher.firstName} ${teacher.lastName}` })),
)

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

function currentFilter(beforeChangeSetId: number | null): HistoryFilter {
  return {
    term: activeTerm.value,
    stream: filterStream.value,
    subjectId: filterSubjectId.value,
    companyId: filterCompanyId.value,
    teacherId: filterTeacherId.value,
    includeOpening: includeOpening.value,
    beforeChangeSetId,
    limit: HISTORY_PAGE_SIZE,
  }
}

async function load(): Promise<void> {
  // Dönem henüz yüklenmediyse (sidebar'ın `loadTerms()` çağrısı sürüyorsa)
  // boş terimle sorgu atılmaz; `watch(activeTerm, load)` gerçek değer
  // geldiğinde zaten yeniden çağırır.
  if (activeTerm.value.length === 0) return
  isLoading.value = true
  try {
    const response = await listHistory(currentFilter(null))
    entries.value = response.entries
    nextBeforeChangeSetId.value = response.nextBeforeChangeSetId
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

async function loadMore(): Promise<void> {
  if (nextBeforeChangeSetId.value === null) return
  isLoadingMore.value = true
  try {
    const response = await listHistory(currentFilter(nextBeforeChangeSetId.value))
    entries.value = [...entries.value, ...response.entries]
    nextBeforeChangeSetId.value = response.nextBeforeChangeSetId
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoadingMore.value = false
  }
}

async function loadFilterOptions(): Promise<void> {
  try {
    const [companyList, teacherList] = await Promise.all([companiesApi.list(), teachersApi.list()])
    companies.value = companyList
    teachers.value = teacherList
    // Filtrede seçili işletme/öğretmen artık listede yoksa (silinmiş, dönem
    // değişmiş) temizlenir; geçerli seçim korunur.
    selection.syncHistoryOptions(
      companyList.map((c) => c.id),
      teacherList.map((t) => t.id),
    )
  } catch (error: unknown) {
    showError(error)
  }
}

// Dönem değişince ya da herhangi bir filtre değişince liste baştan yüklenir.
watch(activeTerm, load)
watch([filterStream, filterSubjectId, filterCompanyId, filterTeacherId, includeOpening], load)

onMounted(() => {
  void loadFilterOptions()
  void load()
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.history-filters { display: flex; flex-wrap: wrap; align-items: flex-end; gap: 1rem; }
.filter-field { display: flex; flex-direction: column; gap: 0.375rem; min-width: 12rem; }
.filter-field label { font-size: 0.875rem; font-weight: 500; }
.filter-field--inline { flex-direction: row; align-items: center; gap: 0.5rem; min-width: 0; }
.history-empty { color: var(--p-text-muted-color); }
.history-list { display: flex; flex-direction: column; gap: 0.75rem; }
</style>
