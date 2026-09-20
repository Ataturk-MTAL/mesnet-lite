<template>
  <div class="page">
    <div class="page-header">
      <h1 class="page-title">{{ labels.nav.importExport }}</h1>
      <Tag v-if="activeTerm" :value="activeTerm" severity="secondary" icon="pi pi-calendar" />
    </div>

    <Message severity="info" :closable="false">{{ labels.importCsv.termNote }}</Message>

    <Card>
      <template #title>{{ labels.importCsv.title }}</template>
      <template #content>
        <div class="file-row">
          <!-- Dosya webview tarafında okunup metin olarak gönderilir; ayrı bir
               dosya sistemi eklentisine ve izin tanımına gerek kalmaz. -->
          <input
            ref="fileInput"
            type="file"
            accept=".csv,text/csv"
            class="file-input"
            @change="onFileChange"
          />
          <Button
            :label="labels.importCsv.chooseFile"
            icon="pi pi-file-import"
            severity="secondary"
            @click="fileInput?.click()"
          />
          <span v-if="fileName" class="file-name">{{ fileName }}</span>
        </div>
        <small class="hint">{{ labels.importCsv.fileHint }}</small>
      </template>
    </Card>

    <Card v-if="preview">
      <template #title>
        {{ labels.importCsv.previewTitle }} —
        {{ preview.groups.length }} {{ labels.importCsv.summaryCompanies }},
        {{ preview.totalStudents }} {{ labels.importCsv.summaryStudents }}
      </template>
      <template #content>
        <DataTable :value="preview.groups" dataKey="key" paginator :rows="15" stripedRows>
          <Column field="companyName" :header="labels.importCsv.companyName" />
          <Column field="addressText" :header="labels.importCsv.address" />

          <Column :header="labels.importCsv.oneWay">
            <template #body="{ data }">{{ formatKm(data.oneWayDistanceKm) }}</template>
          </Column>
          <Column :header="labels.importCsv.roundTrip">
            <template #body="{ data }">{{ formatKm(data.roundTripDistanceKm) }}</template>
          </Column>

          <Column :header="labels.importCsv.students">
            <template #body="{ data }">
              <span :title="data.studentNames.join(', ')">
                {{ data.studentCount }} — {{ data.studentNames.join(', ') }}
              </span>
            </template>
          </Column>

          <Column :header="labels.importCsv.status">
            <template #body="{ data }">
              <Tag
                :value="data.existingCompanyId === null
                  ? labels.importCsv.statusNew
                  : labels.importCsv.statusExisting"
                :severity="data.existingCompanyId === null ? 'success' : 'warn'"
              />
            </template>
          </Column>

          <Column :header="labels.importCsv.policy">
            <template #body="{ data }">
              <Select
                v-if="data.existingCompanyId !== null"
                :model-value="policies[data.key] ?? 'merge'"
                :options="policyOptions"
                optionLabel="label"
                optionValue="value"
                class="policy-select"
                @update:model-value="(value: DuplicatePolicy) => (policies[data.key] = value)"
              />
              <span v-else class="muted">—</span>
            </template>
          </Column>
        </DataTable>

        <Message
          v-if="preview.errors.length > 0"
          severity="warn"
          :closable="false"
          class="errors"
        >
          <strong>{{ labels.importCsv.errorsTitle }} ({{ preview.errors.length }})</strong>
          <ul>
            <li v-for="(error, index) in preview.errors" :key="index">{{ error }}</li>
          </ul>
        </Message>
      </template>
      <template #footer>
        <div class="actions">
          <Button
            :label="labels.importCsv.apply"
            icon="pi pi-check"
            :loading="isApplying"
            @click="applyImport"
          />
        </div>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.studentListImport.title }}</template>
      <template #content>
        <div class="file-row">
          <!-- e-Okul dosyası İKİLİDİR (.xls); metin olarak değil bayt dizisi olarak okunur. -->
          <input
            ref="studentListFileInput"
            type="file"
            accept=".xls,application/vnd.ms-excel"
            multiple
            class="file-input"
            data-testid="student-list-file-input"
            @change="onStudentListFilesChange"
          />
          <Button
            :label="labels.studentListImport.chooseFiles"
            icon="pi pi-file-import"
            severity="secondary"
            @click="studentListFileInput?.click()"
          />
          <Button
            :label="labels.studentListImport.previewButton"
            icon="pi pi-eye"
            :disabled="studentListFiles.length === 0"
            :loading="isPreviewingStudentList"
            data-testid="student-list-preview-button"
            @click="previewStudentList"
          />
        </div>
        <small class="hint">{{ labels.studentListImport.fileHint }}</small>

        <div v-if="studentListFiles.length > 0" class="selected-files">
          <span class="selected-files-label">{{ labels.studentListImport.selectedFilesLabel }}</span>
          <div class="chip-row">
            <Chip
              v-for="file in studentListFiles"
              :key="file.name"
              :label="file.name"
              removable
              @remove="removeStudentListFile(file.name)"
            />
          </div>
        </div>
      </template>
    </Card>

    <Card v-if="studentListPreview">
      <template #title>{{ labels.studentListImport.previewTitle }}</template>
      <template #content>
        <div v-for="classPreview in studentListPreview.classes" :key="classPreview.fileName" class="class-preview">
          <div class="class-preview-header">
            <span class="class-preview-file">{{ classPreview.fileName }}</span>
            <Tag :value="classPreview.grade" severity="secondary" />
            <Tag :value="classPreview.fieldName" severity="secondary" />
            <Tag :value="`${classPreview.newCount} ${labels.studentListImport.statusNew}`" severity="success" />
            <Tag :value="`${classPreview.changedCount} ${labels.studentListImport.statusChanged}`" severity="warn" />
            <Tag :value="`${classPreview.unchangedCount} ${labels.studentListImport.statusUnchanged}`" severity="secondary" />
          </div>

          <!-- `dataKey` verilmez: öğrenci no e-Okul dosyasında boş olabilir (Option<String>),
               bu yüzden benzersizliği garanti etmez; salt okunur bir tabloda gerekli de değildir. -->
          <DataTable :value="classPreview.rows" paginator :rows="15" stripedRows>
            <Column :header="labels.studentListImport.studentNo">
              <template #body="{ data }">{{ data.studentNo ?? '—' }}</template>
            </Column>
            <Column :header="labels.studentListImport.studentName">
              <template #body="{ data }">
                <div class="student-name-cell">
                  <span>{{ data.firstName }} {{ data.lastName }}</span>
                  <small v-if="data.status === 'changed' && data.previous" class="previous-hint">
                    {{
                      labels.studentListImport.previousValue(
                        data.previous.firstName,
                        data.previous.lastName,
                        data.previous.grade,
                        data.previous.branch,
                      )
                    }}
                  </small>
                </div>
              </template>
            </Column>
            <Column field="branch" :header="labels.studentListImport.branch" />
            <Column :header="labels.studentListImport.status">
              <template #body="{ data }">
                <Tag :value="studentRowStatusLabel(data.status)" :severity="studentRowStatusSeverity(data.status)" />
              </template>
            </Column>
          </DataTable>
        </div>

        <Message
          v-if="studentListPreview.warnings.length > 0"
          severity="warn"
          :closable="false"
          class="errors"
        >
          <strong>{{ labels.studentListImport.warningsTitle }} ({{ studentListPreview.warnings.length }})</strong>
          <ul>
            <li v-for="(warning, index) in studentListPreview.warnings" :key="index">{{ warning }}</li>
          </ul>
        </Message>

        <Message v-if="studentListSummary" severity="success" :closable="false">
          {{ studentListSummary.created }} {{ labels.importCsv.resultCreated }},
          {{ studentListSummary.updated }} {{ labels.importCsv.resultUpdated }},
          {{ studentListSummary.skipped }} {{ labels.importCsv.resultSkipped }}
        </Message>
        <Message
          v-if="studentListSummary && studentListSummary.warnings.length > 0"
          severity="warn"
          :closable="false"
          class="errors"
        >
          <strong>{{ labels.studentListImport.warningsTitle }} ({{ studentListSummary.warnings.length }})</strong>
          <ul>
            <li v-for="(warning, index) in studentListSummary.warnings" :key="index">{{ warning }}</li>
          </ul>
        </Message>
      </template>
      <template #footer>
        <div class="actions">
          <Button
            :label="labels.studentListImport.apply"
            icon="pi pi-check"
            :loading="isLoadingChangeTerm || isApplyingStudentList"
            data-testid="student-list-apply-button"
            @click="openStudentListApplyDialog"
          />
        </div>
      </template>
    </Card>

    <ChangeDetailsDialog
      :visible="isStudentListApplyDialogOpen"
      :term="studentListChangeTerm"
      :title="labels.history.changeDetailsTitle"
      :effective-date="null"
      reason=""
      @confirm="onStudentListApplyConfirm"
      @cancel="isStudentListApplyDialogOpen = false"
    />

    <Card>
      <template #title>{{ labels.export.title }}</template>
      <template #content>
        <Button
          :label="labels.export.button"
          icon="pi pi-file-excel"
          severity="secondary"
          outlined
          :loading="isExporting"
          @click="exportExcel"
        />
        <small class="hint">{{ labels.export.note }}</small>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.nav.groupReports }}</template>
      <template #content>
        <div class="report-row">
          <div class="report-actions">
            <Button
              :label="labels.reports.commissionMinutesPdf"
              :aria-label="labels.reports.commissionMinutesPdf"
              icon="pi pi-file-pdf"
              severity="secondary"
              outlined
              data-testid="commission-minutes-pdf-button"
              :loading="isPrintingMinutesPdf"
              @click="exportCommissionMinutesPdf"
            />
            <Button
              :label="labels.reports.commissionMinutesXlsx"
              :aria-label="labels.reports.commissionMinutesXlsx"
              icon="pi pi-file-excel"
              severity="secondary"
              outlined
              data-testid="commission-minutes-xlsx-button"
              :loading="isExportingMinutesXlsx"
              @click="exportCommissionMinutesXlsx"
            />
          </div>
          <small class="hint">{{ labels.reports.commissionMinutesNote }}</small>
        </div>

        <div class="report-row">
          <Button
            :label="labels.reports.assignmentSheet"
            icon="pi pi-file-pdf"
            severity="secondary"
            outlined
            :loading="isPrintingSheet"
            @click="exportAssignmentSheet"
          />
          <small class="hint">{{ labels.reports.assignmentSheetNote }}</small>
        </div>

        <div class="report-row">
          <Button
            :label="labels.reports.visitLists"
            icon="pi pi-file-pdf"
            severity="secondary"
            outlined
            :loading="isPrintingVisits"
            @click="exportVisitLists"
          />
          <small class="hint">{{ labels.reports.visitListsNote }}</small>
        </div>
      </template>
    </Card>

    <Message v-if="summary" severity="success" :closable="false">
      {{ summary.companiesCreated }} {{ labels.importCsv.summaryCompanies }}
      {{ labels.importCsv.resultCreated }},
      {{ summary.companiesMatched }} {{ labels.importCsv.resultMatched }},
      {{ summary.companiesUpdated }} {{ labels.importCsv.resultUpdated }},
      {{ summary.companiesSkipped }} {{ labels.importCsv.resultSkipped }} ·
      {{ summary.studentsCreated }} {{ labels.importCsv.summaryStudents }}
      {{ labels.importCsv.resultCreated }},
      {{ summary.studentsSkipped }} {{ labels.importCsv.resultSkipped }}
    </Message>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { importApi } from '../api/importExport'
import type { DuplicatePolicy, ImportPreview, ImportSummary } from '../api/importExport'
import {
  readFileAsBytes,
  studentListImportApi,
  type StudentListFile,
  type StudentListPreview,
  type StudentListRowStatus,
  type StudentListSummary,
} from '../api/studentListImport'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'
import { filesApi } from '../api/files'
import { listTermsWithDates } from '../api/terms'
import ChangeDetailsDialog from '../components/history/ChangeDetailsDialog.vue'
import type { TermWithDates } from '../types/models'

const toast = useToast()

const fileInput = ref<HTMLInputElement | null>(null)
const fileName = ref('')
const fileContent = ref('')
const preview = ref<ImportPreview | null>(null)
const summary = ref<ImportSummary | null>(null)
const isApplying = ref(false)
const isExporting = ref(false)
const isPrintingSheet = ref(false)
const isPrintingVisits = ref(false)
const isPrintingMinutesPdf = ref(false)
const isExportingMinutesXlsx = ref(false)

async function exportExcel(): Promise<void> {
  isExporting.value = true
  try {
    const name = `MESNET-${activeTerm.value.replace('/', '-')}.xlsx`
    const path = await filesApi.exportWorkbook(name)
    toast.add({ severity: 'success', summary: labels.export.saved, detail: path, life: 8000 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    isExporting.value = false
  }
}

/** Rapor üretimi ile diske yazmayı tek yerde birleştirir. */
async function saveReport(
  busy: typeof isPrintingSheet,
  produce: (fileName: string) => Promise<string>,
  baseName: string,
  extension: 'pdf' | 'xlsx' = 'pdf',
): Promise<void> {
  busy.value = true
  try {
    const path = await produce(`${baseName}-${activeTerm.value.replace('/', '-')}.${extension}`)
    toast.add({ severity: 'success', summary: labels.export.saved, detail: path, life: 8000 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    busy.value = false
  }
}

function exportCommissionMinutesPdf(): void {
  void saveReport(
    isPrintingMinutesPdf,
    filesApi.exportCommissionMinutesPdf,
    'Isletme-Belirleme-Komisyon-Tutanagi',
  )
}

function exportCommissionMinutesXlsx(): void {
  void saveReport(
    isExportingMinutesXlsx,
    filesApi.exportCommissionMinutesXlsx,
    'Isletme-Belirleme-Komisyon-Tutanagi',
    'xlsx',
  )
}

function exportAssignmentSheet(): void {
  void saveReport(isPrintingSheet, filesApi.exportAssignmentSheet, 'Gorevlendirme-Cizelgesi')
}

function exportVisitLists(): void {
  void saveReport(isPrintingVisits, filesApi.exportVisitLists, 'Ziyaret-Listeleri')
}

// Yalnızca mevcut kayıtla çakışan gruplar için anlamlıdır; belirtilmeyen
// çakışmalar Rust tarafında 'merge' sayılır.
const policies = reactive<Record<string, DuplicatePolicy>>({})

const policyOptions = [
  { value: 'merge' as const, label: labels.importCsv.policyMerge },
  { value: 'update' as const, label: labels.importCsv.policyUpdate },
  { value: 'skip' as const, label: labels.importCsv.policySkip },
]

function formatKm(value: number | null): string {
  return value === null ? '—' : value.toFixed(1)
}

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

async function onFileChange(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return

  fileName.value = file.name
  summary.value = null
  Object.keys(policies).forEach((key) => delete policies[key])

  try {
    // JotForm dosyası UTF-8; BOM Rust tarafında kırpılır.
    fileContent.value = await file.text()
    preview.value = await importApi.preview(fileContent.value)
  } catch (error: unknown) {
    preview.value = null
    showError(error)
  }
}

async function applyImport(): Promise<void> {
  if (!fileContent.value) {
    toast.add({ severity: 'warn', summary: labels.importCsv.noFile, life: 4000 })
    return
  }

  isApplying.value = true
  try {
    summary.value = await importApi.apply(fileContent.value, { ...policies })
    toast.add({ severity: 'success', summary: labels.importCsv.applied, life: 4000 })
    // Önizleme artık eskidir; yeniden çalıştırıp güncel durumu göster.
    preview.value = await importApi.preview(fileContent.value)
  } catch (error: unknown) {
    showError(error)
  } finally {
    isApplying.value = false
  }
}

// ---------------------------------------------------------------------------
// e-Okul sınıf listesi (.XLS) içe aktarımı — CSV akışından bağımsız, ayrı bir
// dosya kümesi ve önizleme/uygula döngüsü kullanır.
// ---------------------------------------------------------------------------

const studentListFileInput = ref<HTMLInputElement | null>(null)
const studentListFiles = ref<StudentListFile[]>([])
const studentListPreview = ref<StudentListPreview | null>(null)
const studentListSummary = ref<StudentListSummary | null>(null)
const isPreviewingStudentList = ref(false)
const isApplyingStudentList = ref(false)
const isLoadingChangeTerm = ref(false)
const isStudentListApplyDialogOpen = ref(false)

// Dönem bilgisi yalnız "İçe Aktar" tıklanınca yüklenir; sayfa açılışında değil —
// aksi hâlde bu görünümdeki diğer testler beklenmedik bir çağrı görür.
const termsWithDates = ref<TermWithDates[]>([])

/** Diyalog `TermWithDates` zorunlu kılar; dönem henüz yüklenmediyse zararsız bir yer tutucu döner. */
const studentListChangeTerm = computed<TermWithDates>(
  () =>
    termsWithDates.value.find((t) => t.term === activeTerm.value) ?? {
      term: '',
      startDate: '',
      endDate: '',
      datesConfirmed: false,
      isPlanning: true,
      defaultAsOf: '',
      earliestAllowedDate: '',
    },
)

function studentRowStatusLabel(status: StudentListRowStatus): string {
  if (status === 'new') return labels.studentListImport.statusNew
  if (status === 'changed') return labels.studentListImport.statusChanged
  return labels.studentListImport.statusUnchanged
}

function studentRowStatusSeverity(status: StudentListRowStatus): 'success' | 'warn' | 'secondary' {
  if (status === 'new') return 'success'
  if (status === 'changed') return 'warn'
  return 'secondary'
}

/** Yeni dosya seçimi/kaldırma önceki önizleme ve sonucu geçersizleştirir. */
function invalidateStudentListPreview(): void {
  studentListPreview.value = null
  studentListSummary.value = null
}

function removeStudentListFile(name: string): void {
  studentListFiles.value = studentListFiles.value.filter((file) => file.name !== name)
  invalidateStudentListPreview()
}

async function onStudentListFilesChange(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const chosen = Array.from(input.files ?? [])
  input.value = ''
  if (chosen.length === 0) return

  const results = await Promise.allSettled(chosen.map((file) => readFileAsBytes(file)))
  const succeeded = results
    .filter((result): result is PromiseFulfilledResult<StudentListFile> => result.status === 'fulfilled')
    .map((result) => result.value)
  const failedNames = chosen
    .filter((_, index) => results[index].status === 'rejected')
    .map((file) => file.name)

  if (succeeded.length > 0) {
    // Aynı ada sahip yeni bir seçim, listedeki önceki halinin yerine geçer.
    const newNames = new Set(succeeded.map((file) => file.name))
    const kept = studentListFiles.value.filter((file) => !newNames.has(file.name))
    studentListFiles.value = [...kept, ...succeeded]
    invalidateStudentListPreview()
  }

  if (failedNames.length > 0) {
    toast.add({
      severity: 'error',
      summary: labels.common.error,
      detail: labels.studentListImport.fileReadError(failedNames.join(', ')),
      life: 8000,
    })
  }
}

async function previewStudentList(): Promise<void> {
  if (studentListFiles.value.length === 0) return

  isPreviewingStudentList.value = true
  studentListSummary.value = null
  try {
    studentListPreview.value = await studentListImportApi.preview(studentListFiles.value)
  } catch (error: unknown) {
    studentListPreview.value = null
    showError(error)
  } finally {
    isPreviewingStudentList.value = false
  }
}

/** Dönem tarihlerini (bir kez) yükler ve yürürlük tarihi/gerekçe penceresini açar. */
async function openStudentListApplyDialog(): Promise<void> {
  isLoadingChangeTerm.value = true
  try {
    if (termsWithDates.value.length === 0) {
      termsWithDates.value = await listTermsWithDates()
    }
    isStudentListApplyDialogOpen.value = true
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoadingChangeTerm.value = false
  }
}

async function onStudentListApplyConfirm(details: { effectiveDate: string | null; reason: string }): Promise<void> {
  isStudentListApplyDialogOpen.value = false

  isApplyingStudentList.value = true
  try {
    studentListSummary.value = await studentListImportApi.apply(
      studentListFiles.value,
      details.effectiveDate,
      details.reason,
    )
    toast.add({ severity: 'success', summary: labels.studentListImport.applied, life: 4000 })
    // Önizleme artık eskidir; yeniden çalıştırıp güncel durumu göster.
    studentListPreview.value = await studentListImportApi.preview(studentListFiles.value)
  } catch (error: unknown) {
    showError(error)
  } finally {
    isApplyingStudentList.value = false
  }
}
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; gap: 0.75rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.file-row { display: flex; align-items: center; gap: 1rem; }
.file-input { display: none; }
.file-name { font-size: 0.875rem; color: var(--p-text-muted-color); }
.report-row { display: flex; flex-direction: column; align-items: flex-start; gap: 0.375rem; margin-bottom: 1rem; }
.report-row:last-child { margin-bottom: 0; }
.report-row .hint { margin-top: 0; }
.report-actions { display: flex; flex-wrap: wrap; gap: 0.75rem; }
.hint { display: block; margin-top: 0.5rem; color: var(--p-text-muted-color); font-size: 0.75rem; }
.policy-select { min-width: 12rem; }
.muted { color: var(--p-text-muted-color); }
.errors { margin-top: 1rem; }
.errors ul { margin: 0.5rem 0 0; padding-left: 1.25rem; }
.actions { display: flex; justify-content: flex-end; }
.selected-files { margin-top: 0.75rem; display: flex; flex-direction: column; gap: 0.5rem; }
.selected-files-label { font-size: 0.75rem; font-weight: 500; color: var(--p-text-muted-color); }
.chip-row { display: flex; flex-wrap: wrap; gap: 0.5rem; }
.class-preview { margin-bottom: 1.5rem; }
.class-preview:last-child { margin-bottom: 0; }
.class-preview-header { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin-bottom: 0.75rem; }
.class-preview-file { font-weight: 600; }
.student-name-cell { display: flex; flex-direction: column; gap: 0.125rem; }
.previous-hint { color: var(--p-text-muted-color); }
</style>
