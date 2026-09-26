<template>
  <Card>
    <template #title>{{ labels.versions.title }}</template>
    <template #content>
      <div class="versions-toolbar">
        <small class="hint">{{ labels.versions.note }}</small>
        <Button
          :label="labels.versions.save"
          icon="pi pi-save"
          severity="secondary"
          outlined
          data-testid="version-save-button"
          @click="isSaveDialogOpen = true"
        />
      </div>

      <DataTable :value="versions" :loading="isLoading" dataKey="id" stripedRows :rowClass="rowClass">
        <template #empty>{{ labels.versions.empty }}</template>

        <Column field="name" :header="labels.versions.name" />
        <Column field="term" :header="labels.versions.term" />

        <Column :header="labels.versions.date">
          <template #body="{ data }: { data: Version }">{{ formatVersionTimestamp(data.createdAt) }}</template>
        </Column>

        <Column :header="labels.versions.kind">
          <template #body="{ data }: { data: Version }">
            <Tag
              :value="data.kind === 'auto' ? labels.versions.kindAuto : labels.versions.kindManual"
              :severity="data.kind === 'auto' ? 'secondary' : 'info'"
            />
          </template>
        </Column>

        <Column :header="labels.versions.actions">
          <template #body="{ data }: { data: Version }">
            <div class="row-actions">
              <Button
                :label="labels.versions.exportAction"
                icon="pi pi-download"
                size="small"
                severity="secondary"
                outlined
                :disabled="!data.isAvailable"
                :loading="exportingVersionId === data.id"
                :aria-label="data.isAvailable ? labels.versions.exportAction : labels.versions.unavailable"
                v-tooltip.top="data.isAvailable ? undefined : labels.versions.unavailable"
                data-testid="version-export-button"
                @click="openExportMenu($event, data)"
              />
              <Button
                icon="pi pi-trash"
                size="small"
                severity="danger"
                text
                :aria-label="labels.common.delete"
                v-tooltip.top="labels.common.delete"
                data-testid="version-delete-button"
                @click="confirmDelete(data)"
              />
            </div>
          </template>
        </Column>
      </DataTable>
    </template>
  </Card>

  <Menu ref="exportMenuRef" :model="exportMenuItems" :popup="true" />

  <SaveVersionDialog v-model:visible="isSaveDialogOpen" :saving="isSavingVersion" @save="handleSaveVersion" />
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import type { MenuItem } from 'openvue/menuitem'
import type { MenuMethods } from 'openvue/menu'
import { versionsApi } from '../../api/versions'
import { filesApi } from '../../api/files'
import { labels } from '../../i18n/labels'
import { formatVersionTimestamp, versionDateForFileName } from '../../utils/versionDate'
import type { ExportTrigger, Version } from '../../types/models'
import SaveVersionDialog from './SaveVersionDialog.vue'

/** Bir çıktı türünün, sürümden üretim için gerekli tüm bilgisi. */
interface ReportDescriptor {
  trigger: ExportTrigger
  label: string
  baseName: string
  extension: 'pdf' | 'xlsx'
  exportFn: (fileName: string, versionId: number | null) => Promise<string>
}

// Mevcut dosya adı deseniyle birebir aynı temel adlar (bkz. ImportExportView.vue).
const REPORT_DESCRIPTORS: ReportDescriptor[] = [
  {
    trigger: 'assignmentSheet',
    label: labels.reports.assignmentSheet,
    baseName: 'Gorevlendirme-Cizelgesi',
    extension: 'pdf',
    exportFn: filesApi.exportAssignmentSheet,
  },
  {
    trigger: 'visitLists',
    label: labels.reports.visitLists,
    baseName: 'Ziyaret-Listeleri',
    extension: 'pdf',
    exportFn: filesApi.exportVisitLists,
  },
  {
    trigger: 'commissionMinutesPdf',
    label: labels.reports.commissionMinutesPdf,
    baseName: 'Isletme-Belirleme-Komisyon-Tutanagi',
    extension: 'pdf',
    exportFn: filesApi.exportCommissionMinutesPdf,
  },
  {
    trigger: 'commissionMinutesXlsx',
    label: labels.reports.commissionMinutesXlsx,
    baseName: 'Isletme-Belirleme-Komisyon-Tutanagi',
    extension: 'xlsx',
    exportFn: filesApi.exportCommissionMinutesXlsx,
  },
  {
    trigger: 'workbook',
    label: labels.export.button,
    baseName: 'MESNET',
    extension: 'xlsx',
    exportFn: filesApi.exportWorkbook,
  },
]

const toast = useToast()
const confirm = useConfirm()

const versions = ref<Version[]>([])
const isLoading = ref(false)
const isSaveDialogOpen = ref(false)
const isSavingVersion = ref(false)
/** Menüden bir çıktı seçilip beklenirken hangi sürümün düğmesi meşgul görünsün. */
const exportingVersionId = ref<number | null>(null)

const exportMenuRef = ref<MenuMethods | null>(null)
const activeVersion = ref<Version | null>(null)

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

async function load(): Promise<void> {
  isLoading.value = true
  try {
    versions.value = await versionsApi.list()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

onMounted(load)

function rowClass(data: Version): Record<string, boolean> {
  return { 'row-unavailable': !data.isAvailable }
}

/** Bir sürümden tek bir çıktı üretir ve indirilenler klasörüne kaydeder. */
async function exportVersion(version: Version, descriptor: ReportDescriptor): Promise<void> {
  exportingVersionId.value = version.id
  try {
    const versionSuffix = `surum-${versionDateForFileName(version.createdAt)}`
    const fileName = `${descriptor.baseName}-${version.term.replace('/', '-')}-${versionSuffix}.${descriptor.extension}`
    const path = await descriptor.exportFn(fileName, version.id)
    toast.add({ severity: 'success', summary: labels.export.saved, detail: path, life: 8000 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    exportingVersionId.value = null
  }
}

const exportMenuItems = computed<MenuItem[]>(() => {
  const version = activeVersion.value
  if (!version) return []
  return REPORT_DESCRIPTORS.map((descriptor) => ({
    label: descriptor.label,
    command: () => void exportVersion(version, descriptor),
  }))
})

function openExportMenu(event: Event, version: Version): void {
  if (!version.isAvailable) return
  activeVersion.value = version
  exportMenuRef.value?.toggle(event)
}

function confirmDelete(version: Version): void {
  confirm.require({
    message: labels.versions.deleteConfirm,
    header: version.name,
    acceptLabel: labels.common.yes,
    rejectLabel: labels.common.no,
    acceptProps: { severity: 'danger' },
    accept: async () => {
      try {
        await versionsApi.remove(version.id)
        toast.add({ severity: 'success', summary: labels.versions.deleted, life: 2500 })
        await load()
      } catch (error: unknown) {
        showError(error)
      }
    },
  })
}

async function handleSaveVersion(name: string): Promise<void> {
  isSavingVersion.value = true
  try {
    await versionsApi.create(name)
    toast.add({ severity: 'success', summary: labels.versions.saved, life: 2500 })
    isSaveDialogOpen.value = false
    await load()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSavingVersion.value = false
  }
}

// Normal (sürümsüz) bir çıktı alındıktan sonra yeni bir otomatik sürüm
// oluşmuş olabilir; çağıran görünüm bu metotla listeyi tazeler.
defineExpose({ reload: load })
</script>

<style scoped>
.versions-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1rem;
}
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
.row-actions { display: flex; gap: 0.5rem; }

/* Veritabanı kopyası artık olmayan sürümler görsel olarak soluklaştırılır. */
:deep(.row-unavailable) { opacity: 0.55; }
</style>
