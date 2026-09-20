<template>
  <div class="page">
    <div class="page-header">
      <div class="title-group">
        <h1 class="page-title">{{ labels.teachingLoad.title }}</h1>
        <Tag v-if="board?.term" :value="board.term" severity="secondary" icon="pi pi-calendar" />
      </div>
      <div class="header-actions">
        <Button
          :label="labels.teachingLoad.addRow"
          icon="pi pi-plus"
          severity="secondary"
          outlined
          @click="addRow"
        />
        <Button
          :label="labels.teachingLoad.save"
          icon="pi pi-check"
          :disabled="!isDirty"
          :loading="isSaving"
          @click="save"
        />
      </div>
    </div>

    <Message severity="secondary" :closable="false">{{ labels.teachingLoad.subtitle }}</Message>

    <Message
      v-for="(error, index) in validationErrors"
      :key="index"
      severity="error"
      :closable="false"
    >
      {{ error }}
    </Message>

    <!-- Havuz özeti -->
    <Card>
      <template #content>
        <div class="summary">
          <div class="summary-item">
            <div class="summary-value" data-test="branch-hours">{{ liveBranchHours }}</div>
            <div class="summary-label">{{ labels.teachingLoad.branchHours }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value" data-test="chief-hours">{{ chiefPlanningHours }}</div>
            <div class="summary-label">{{ labels.teachingLoad.chiefHours }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value" data-test="pool-hours">{{ livePoolHours }}</div>
            <div class="summary-label">{{ labels.teachingLoad.pool }}</div>
          </div>
        </div>
        <small v-if="isDirty" class="hint">{{ labels.teachingLoad.poolHint }}</small>
      </template>
    </Card>

    <DataTable
      :value="draftRows"
      :loading="isLoading"
      dataKey="key"
      stripedRows
      :rowClass="rowClass"
    >
      <template #empty>{{ labels.teachingLoad.empty }}</template>

      <Column :header="labels.teachingLoad.grade">
        <template #body="{ data }: { data: DraftRow }">
          <InputText
            :model-value="data.grade"
            :aria-label="labels.teachingLoad.grade"
            @update:model-value="(value: string | undefined) => updateRow(data.key, { grade: value ?? '' })"
          />
        </template>
      </Column>

      <Column :header="labels.teachingLoad.branch">
        <template #body="{ data }: { data: DraftRow }">
          <InputText
            :model-value="data.branch"
            :aria-label="labels.teachingLoad.branch"
            @update:model-value="(value: string | undefined) => updateRow(data.key, { branch: value ?? '' })"
          />
        </template>
      </Column>

      <Column :header="labels.teachingLoad.weeklyHours">
        <template #body="{ data }: { data: DraftRow }">
          <InputNumber
            :model-value="data.weeklyHours"
            :min="0"
            showButtons
            buttonLayout="horizontal"
            class="hours-input"
            :aria-label="labels.teachingLoad.weeklyHours"
            @update:model-value="(value: number | null) => updateRow(data.key, { weeklyHours: value ?? 0 })"
          />
        </template>
      </Column>

      <Column :header="labels.teachingLoad.groupCount">
        <template #body="{ data }: { data: DraftRow }">
          <InputNumber
            :model-value="data.groupCount"
            :min="0"
            showButtons
            buttonLayout="horizontal"
            class="hours-input"
            :aria-label="labels.teachingLoad.groupCount"
            @update:model-value="(value: number | null) => updateRow(data.key, { groupCount: value ?? 0 })"
          />
        </template>
      </Column>

      <Column :header="labels.teachingLoad.contribution">
        <template #body="{ data }: { data: DraftRow }">
          <strong>{{ data.weeklyHours * data.groupCount }}</strong>
        </template>
      </Column>

      <Column :header="labels.teachingLoad.status">
        <template #body="{ data }: { data: DraftRow }">
          <Tag
            v-if="data.isSuggested"
            :value="labels.teachingLoad.suggested"
            severity="warn"
            v-tooltip.top="labels.teachingLoad.suggestedTooltip"
          />
        </template>
      </Column>

      <Column>
        <template #body="{ data }: { data: DraftRow }">
          <Button
            icon="pi pi-trash"
            severity="danger"
            outlined
            rounded
            size="small"
            :aria-label="labels.teachingLoad.removeRow"
            v-tooltip.top="labels.teachingLoad.removeRow"
            @click="removeRow(data.key)"
          />
        </template>
      </Column>
    </DataTable>

    <div class="footer-actions">
      <span class="muted">{{ labels.teachingLoad.savedHint }}</span>
      <RouterLink to="/company-hours">
        <Button
          :label="labels.teachingLoad.goToHours"
          icon="pi pi-arrow-right"
          severity="secondary"
          outlined
        />
      </RouterLink>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import { teachingLoadApi } from '../api/teachingLoad'
import type { TeachingLoadBoard, TeachingLoadInput, TeachingLoadRow } from '../api/teachingLoad'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'

/** Ekranda düzenlenen satır. `key` yalnızca yerel `v-for` kimliği içindir,
 *  sunucuya gitmez — `id === null` olan hem öneri hem de yeni eklenen boş
 *  satırlarda aynı anahtar (bileşik ad) çakışabileceği için stabil bir
 *  string gerekir. */
interface DraftRow extends TeachingLoadRow {
  key: string
}

const toast = useToast()
const confirm = useConfirm()

const board = ref<TeachingLoadBoard | null>(null)
const draftRows = ref<DraftRow[]>([])
const validationErrors = ref<string[]>([])

const isLoading = ref(false)
const isSaving = ref(false)

let keyCounter = 0
function nextKey(): string {
  keyCounter += 1
  return `row-${keyCounter}`
}

function toDraftRows(rows: TeachingLoadRow[]): DraftRow[] {
  return rows.map((row) => ({ ...row, key: nextKey() }))
}

/** Sıra bağımsız karşılaştırma imzası — satır kimliği yerine içeriğe bakar,
 *  çünkü kullanıcı sınıf/dal metnini de değiştirebilir. */
function rowSignature(row: Pick<TeachingLoadRow, 'grade' | 'branch' | 'weeklyHours' | 'groupCount'>): string {
  return `${row.grade.trim().toLocaleLowerCase('tr')}|${row.branch.trim().toLocaleLowerCase('tr')}|${row.weeklyHours}|${row.groupCount}`
}

const isDirty = computed(() => {
  const saved = (board.value?.rows ?? []).map(rowSignature).sort()
  const draft = draftRows.value.map(rowSignature).sort()
  if (saved.length !== draft.length) return true
  return saved.some((signature, index) => signature !== draft[index])
})

// Kaydedilene kadar sunucuya gitmeyen canlı ders saati toplamı.
const liveBranchHours = computed(() =>
  draftRows.value.reduce((sum, row) => sum + row.weeklyHours * row.groupCount, 0),
)

// Şeflik saatleri bu ekranda düzenlenmez; sunucudan geldiği gibi gösterilir.
const chiefPlanningHours = computed(() => board.value?.chiefPlanningHours ?? 0)

// Kullanıcının asıl merak ettiği toplam havuz: canlı ders saatleri + şeflik.
const livePoolHours = computed(() => liveBranchHours.value + chiefPlanningHours.value)

function rowClass(data: DraftRow): Record<string, boolean> {
  return { 'row-suggested': data.isSuggested }
}

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

function updateRow(key: string, patch: Partial<Omit<DraftRow, 'key' | 'id' | 'isSuggested'>>): void {
  draftRows.value = draftRows.value.map((row) => (row.key === key ? { ...row, ...patch } : row))
}

function addRow(): void {
  draftRows.value = [
    ...draftRows.value,
    { key: nextKey(), id: null, grade: '', branch: '', weeklyHours: 0, groupCount: 0, isSuggested: false },
  ]
}

function removeRow(key: string): void {
  draftRows.value = draftRows.value.filter((row) => row.key !== key)
}

/** Satırların ad+dal ikilisi için okunabilir etiket; hata mesajında hangi
 *  satırdan bahsedildiği belli olsun diye kullanılır. */
function rowLabel(row: DraftRow, index: number): string {
  const grade = row.grade.trim()
  const branch = row.branch.trim()
  if (grade.length > 0 || branch.length > 0) return `${grade || '—'} / ${branch || '—'}`
  return `${index + 1}.`
}

/** Rust'taki `teaching_load::validate` + yinelenen sınıf/dal kontrolünün
 *  bire bir istemci tarafı karşılığı — kullanıcı hatayı sunucuya gitmeden
 *  görsün diye. */
function validateRows(rows: DraftRow[]): string[] {
  const errors: string[] = []
  const seen = new Set<string>()

  rows.forEach((row, index) => {
    const grade = row.grade.trim()
    const branch = row.branch.trim()
    const label = rowLabel(row, index)

    if (grade.length === 0) errors.push(`${label}: ${labels.teachingLoad.gradeRequired}`)
    if (branch.length === 0) errors.push(`${label}: ${labels.teachingLoad.branchRequired}`)
    if (row.weeklyHours <= 0) errors.push(`${label}: ${labels.teachingLoad.weeklyHoursInvalid}`)
    if (row.groupCount <= 0) errors.push(`${label}: ${labels.teachingLoad.groupCountInvalid}`)

    if (grade.length > 0 && branch.length > 0) {
      const key = `${grade.toLocaleLowerCase('tr')}|${branch.toLocaleLowerCase('tr')}`
      if (seen.has(key)) {
        errors.push(`${labels.teachingLoad.duplicateRow} ${grade} / ${branch}`)
      }
      seen.add(key)
    }
  })

  return errors
}

function applyBoard(next: TeachingLoadBoard): void {
  board.value = next
  draftRows.value = toDraftRows(next.rows)
  validationErrors.value = []
}

async function load(): Promise<void> {
  isLoading.value = true
  try {
    applyBoard(await teachingLoadApi.get())
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

async function save(): Promise<void> {
  const errors = validateRows(draftRows.value)
  if (errors.length > 0) {
    validationErrors.value = errors
    return
  }

  isSaving.value = true
  try {
    const payload: TeachingLoadInput[] = draftRows.value.map((row) => ({
      grade: row.grade.trim(),
      branch: row.branch.trim(),
      weeklyHours: row.weeklyHours,
      groupCount: row.groupCount,
    }))
    applyBoard(await teachingLoadApi.save(payload))
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSaving.value = false
  }
}

// Kaydetmeden dönem değişirse ya da sayfadan çıkılırsa değişiklik kaybolur;
// kullanıcı onaylamadan çıkılmaz.
function confirmDiscard(): Promise<boolean> {
  return new Promise((resolve) => {
    confirm.require({
      message: labels.teachingLoad.unsavedLeaveConfirm,
      header: labels.common.confirm,
      acceptLabel: labels.common.yes,
      rejectLabel: labels.common.no,
      acceptProps: { severity: 'danger' },
      accept: () => resolve(true),
      reject: () => resolve(false),
      onHide: () => resolve(false),
    })
  })
}

onBeforeRouteLeave(async () => {
  if (!isDirty.value) return true
  return await confirmDiscard()
})

// Dönem, üst çubuktaki seçiciden değiştirilir — o zamana kadar seçim zaten
// veritabanına yazılmıştır ve geri alınamaz. Burada "hayır" demek taslağı
// ESKİ dönemde tutup YENİ dönemin üstüne kaydetmek anlamına geleceğinden
// (veri karışması), diğer tüm ekranlarla aynı kurala uyulur: dönem
// değişince taslak sorgusuz tazelenir. Kaydedilmemiş değişikliğe karşı asıl
// koruma `onBeforeRouteLeave` ile sayfadan ÇIKARKEN devreye girer.
watch(activeTerm, load)

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}
.title-group { display: flex; align-items: center; gap: 0.75rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.header-actions { display: flex; gap: 0.5rem; flex-wrap: wrap; }

.summary { display: flex; flex-wrap: wrap; gap: 2.5rem; }
.summary-item { min-width: 7rem; }
.summary-value { font-size: 1.5rem; font-weight: 700; line-height: 1.1; }
.summary-label { font-size: 0.8125rem; color: var(--p-text-muted-color); margin-top: 0.125rem; }

.hours-input { width: 9rem; }
.hint { display: block; margin-top: 0.5rem; color: var(--p-text-muted-color); }
.muted { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.footer-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 1rem;
  flex-wrap: wrap;
}

/* Öneri satırları kayıtlı satırlardan görsel olarak ayrılır. */
:deep(.row-suggested) {
  background: color-mix(in srgb, var(--p-orange-500) 10%, transparent);
}
</style>
