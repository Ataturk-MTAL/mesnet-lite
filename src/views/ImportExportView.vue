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
import { reactive, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { importApi } from '../api/importExport'
import type { DuplicatePolicy, ImportPreview, ImportSummary } from '../api/importExport'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'
import { filesApi } from '../api/files'

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
</style>
