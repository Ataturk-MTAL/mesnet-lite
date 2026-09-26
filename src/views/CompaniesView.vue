<template>
  <div class="page">
    <div class="page-header">
      <h1 class="page-title">{{ labels.company.title }}</h1>
      <div class="header-actions">
        <Button
          :label="labels.geocoding.button"
          icon="pi pi-map"
          severity="secondary"
          outlined
          :loading="isGeocoding"
          v-tooltip.bottom="labels.geocoding.note"
          @click="runGeocoding"
        />
        <Button :label="labels.common.add" icon="pi pi-plus" @click="openCreate" />
      </div>
    </div>

    <Message severity="secondary" :closable="false">{{ labels.company.termNote }}</Message>

    <DataTable
      :value="companies"
      :loading="isLoading"
      paginator
      :rows="15"
      dataKey="id"
      v-model:filters="filters"
      :globalFilterFields="['name', 'addressText', 'contactFirstName', 'contactLastName']"
      sortMode="single"
      stripedRows
    >
      <template #header>
        <InputText
          v-model="companySearch"
          :placeholder="labels.company.searchPlaceholder"
          class="search-input"
        />
      </template>

      <template #empty>{{ labels.company.empty }}</template>

      <Column field="name" :header="labels.company.name" sortable />
      <Column :header="labels.company.contact">
        <template #body="{ data }">
          {{ contactName(data) }}
        </template>
      </Column>
      <Column field="phone" :header="labels.company.phone" />

      <Column field="oneWayDistanceKm" :header="labels.company.oneWayDistance" sortable>
        <template #body="{ data }">
          {{ formatKm(data.oneWayDistanceKm) }}
        </template>
      </Column>

      <Column :header="labels.company.roundTripDistance">
        <template #body="{ data }">
          {{ formatKm(roundTripDistanceKm(data)) }}
        </template>
      </Column>

      <Column :header="labels.company.geocodeStatus">
        <template #body="{ data }">
          <Tag
            :value="labels.geocodeStatus[data.geocodeStatus as GeocodeStatus]"
            :severity="statusSeverity(data.geocodeStatus)"
          />
        </template>
      </Column>

      <Column>
        <template #body="{ data }">
          <div class="row-actions">
            <Button
              icon="pi pi-map-marker"
              severity="secondary"
              outlined
              size="small"
              :aria-label="labels.company.setLocation"
              v-tooltip.top="labels.company.setLocation"
              @click="openLocation(data)"
            />
            <Button
              icon="pi pi-pencil"
              severity="secondary"
              outlined
              size="small"
              :aria-label="labels.common.edit"
              v-tooltip.top="labels.common.edit"
              @click="openEdit(data)"
            />
            <Button
              icon="pi pi-arrow-right-arrow-left"
              severity="secondary"
              outlined
              size="small"
              :aria-label="labels.company.merge"
              v-tooltip.top="labels.company.merge"
              @click="openMerge(data)"
            />
            <Button
              icon="pi pi-trash"
              severity="danger"
              outlined
              size="small"
              :aria-label="labels.common.delete"
              v-tooltip.top="labels.common.delete"
              @click="confirmRemove(data)"
            />
          </div>
        </template>
      </Column>
    </DataTable>

    <CompanyFormDialog v-model:visible="isDialogOpen" :company="selected" @save="handleSave" />

    <CompanyMergeDialog
      v-model:visible="isMergeDialogOpen"
      :source="mergeSource"
      :companies="companies"
      :term="term"
      @confirm="handleMergeConfirm"
    />

    <Dialog
      v-model:visible="isLocationDialogOpen"
      modal
      :header="locationTarget ? locationTarget.name : labels.company.locationDialogTitle"
      :style="{ width: '46rem' }"
    >
      <LocationPickerMap v-model="editedLocation" :fallback-center="schoolCenter ?? undefined" />
      <template #footer>
        <Button :label="labels.common.cancel" severity="secondary" outlined
                @click="isLocationDialogOpen = false" />
        <Button :label="labels.common.save" :disabled="editedLocation === null"
                @click="saveLocation" />
      </template>
    </Dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import type { DataTableFilterMeta } from 'openvue/datatable'
import CompanyFormDialog from '../components/company/CompanyFormDialog.vue'
import CompanyMergeDialog from '../components/company/CompanyMergeDialog.vue'
import LocationPickerMap from '../components/map/LocationPickerMap.vue'
import { companiesApi } from '../api/companies'
import { settingsApi } from '../api/settings'
import { filesApi } from '../api/files'
import { listTermsWithDates } from '../api/terms'
import { labels } from '../i18n/labels'
import { useTermStore } from '../stores/term'
import { roundTripDistanceKm } from '../types/models'
import type { Company, GeocodeStatus, LatLng, NewCompany, TermWithDates } from '../types/models'
import type { CompanyMergeInput } from '../api/companies'
import { useSelectionStore } from '../stores/selection'
import { buildGlobalFilter, extractGlobalFilterValue } from '../utils/dataTableFilters'

const toast = useToast()
const confirm = useConfirm()
const selection = useSelectionStore()
const { companySearch } = storeToRefs(selection)
const { activeTerm } = storeToRefs(useTermStore())

const companies = ref<Company[]>([])
const isLoading = ref(false)
const isGeocoding = ref(false)

/**
 * Nominatim kullanım koşulları gereği saniyede bir istek gönderilir;
 * 28 işletme yaklaşık 30 saniye sürer.
 */
async function runGeocoding(): Promise<void> {
  isGeocoding.value = true
  try {
    const summary = await filesApi.geocodePending()
    const detail = [
      `${summary.resolved} ${labels.geocoding.resolved}`,
      `${summary.failed} ${labels.geocoding.failed}`,
      `${summary.skipped} ${labels.geocoding.skipped}`,
    ].join(' · ')
    toast.add({
      severity: summary.failed > 0 ? 'warn' : 'success',
      summary: labels.geocoding.done,
      detail: summary.failed > 0 ? `${detail}. ${labels.geocoding.failedNote}` : detail,
      life: 10000,
    })
    for (const warning of summary.warnings) {
      toast.add({ severity: 'warn', summary: labels.common.error, detail: warning, life: 8000 })
    }
    await load()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isGeocoding.value = false
  }
}
const isDialogOpen = ref(false)
const selected = ref<Company | null>(null)
// DataTable'ın arama kutusu iki yönlü; store'daki `companySearch` ile senkron
// kalması için OKUNABİLİR + YAZILABİLİR computed olarak sunulur.
const filters = computed<DataTableFilterMeta>({
  get: () => buildGlobalFilter(companySearch.value),
  set: (next) => {
    companySearch.value = extractGlobalFilterValue(next)
  },
})

const isLocationDialogOpen = ref(false)
const locationTarget = ref<Company | null>(null)
const editedLocation = ref<LatLng | null>(null)
// Konumu olmayan işletme için harita okul konumuna odaklanır.
const schoolCenter = ref<LatLng | null>(null)

const isMergeDialogOpen = ref(false)
const mergeSource = ref<Company | null>(null)
/** Aktif dönemin tarihleri; birleştirmenin yürürlük tarihi/gerekçe penceresi bunu kullanır. */
const term = ref<TermWithDates | null>(null)

function formatKm(value: number | null): string {
  return value === null ? '—' : value.toFixed(1)
}

// Yetkili adı boşsa tabloda diğer boş alanlarla (mesafe vb.) tutarlı olacak
// şekilde tire gösterilir.
function contactName(company: Company): string {
  const fullName = `${company.contactFirstName} ${company.contactLastName}`.trim()
  return fullName === '' ? '—' : fullName
}

function statusSeverity(status: GeocodeStatus): string {
  const map: Record<GeocodeStatus, string> = {
    pending: 'secondary',
    resolved: 'success',
    failed: 'danger',
    manual: 'info',
  }
  return map[status]
}

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

async function load(): Promise<void> {
  isLoading.value = true
  try {
    companies.value = await companiesApi.list()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

function openCreate(): void {
  selected.value = null
  isDialogOpen.value = true
}

function openEdit(company: Company): void {
  selected.value = company
  isDialogOpen.value = true
}

async function handleSave(input: NewCompany): Promise<void> {
  try {
    if (selected.value) {
      await companiesApi.update(selected.value.id, input)
    } else {
      await companiesApi.create(input)
    }
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
    await load()
  } catch (error: unknown) {
    showError(error)
  }
}

function confirmRemove(company: Company): void {
  confirm.require({
    message: labels.common.deleteConfirm,
    header: company.name,
    acceptLabel: labels.common.yes,
    rejectLabel: labels.common.no,
    acceptProps: { severity: 'danger' },
    accept: async () => {
      try {
        const removal = await companiesApi.remove(company.id)
        if (removal.softDeleted) {
          toast.add({
            severity: 'info',
            summary: labels.company.deletedSoftSummary,
            detail: labels.company.deletedSoftDetail,
            life: 6000,
          })
        } else {
          toast.add({ severity: 'success', summary: labels.common.deleted, life: 2500 })
        }
        await load()
      } catch (error: unknown) {
        showError(error)
      }
    },
  })
}

function openLocation(company: Company): void {
  locationTarget.value = company
  editedLocation.value =
    company.latitude !== null && company.longitude !== null
      ? { latitude: company.latitude, longitude: company.longitude }
      : null
  isLocationDialogOpen.value = true
}

async function saveLocation(): Promise<void> {
  const target = locationTarget.value
  const point = editedLocation.value
  if (!target || !point) return

  try {
    await companiesApi.setLocation(target.id, point.latitude, point.longitude)
    toast.add({ severity: 'success', summary: labels.company.locationSaved, life: 2500 })
    isLocationDialogOpen.value = false
    await load()
  } catch (error: unknown) {
    showError(error)
  }
}

function openMerge(company: Company): void {
  mergeSource.value = company
  isMergeDialogOpen.value = true
}

/** Aktif dönemin tarihlerini yükler; birleştirme penceresi buna göre tarih ister. */
async function loadTerm(): Promise<void> {
  try {
    const allTerms = await listTermsWithDates()
    term.value = allTerms.find((t) => t.term === activeTerm.value) ?? null
  } catch (error: unknown) {
    showError(error)
  }
}

async function handleMergeConfirm(payload: CompanyMergeInput): Promise<void> {
  try {
    const summary = await companiesApi.applyMerge(payload)
    const detail = summary.endedCoordination
      ? `${labels.companyMerge.summary(summary.movedStudents, summary.clearedHours)} ${labels.companyMerge.coordinationEndedNote}`
      : labels.companyMerge.summary(summary.movedStudents, summary.clearedHours)
    toast.add({
      severity: summary.warnings.length > 0 ? 'warn' : 'success',
      summary: labels.common.saved,
      detail,
      life: 6000,
    })
    for (const warning of summary.warnings) {
      toast.add({ severity: 'warn', summary: labels.common.error, detail: warning, life: 8000 })
    }
    await load()
  } catch (error: unknown) {
    showError(error)
  }
}

async function loadSchoolCenter(): Promise<void> {
  try {
    const settings = await settingsApi.get()
    const latitude = Number.parseFloat(settings.school_latitude ?? '')
    const longitude = Number.parseFloat(settings.school_longitude ?? '')
    if (!Number.isNaN(latitude) && !Number.isNaN(longitude)) {
      schoolCenter.value = { latitude, longitude }
    }
  } catch {
    // Okul konumu okunamazsa harita varsayılan merkeze düşer; bu bir hata değil.
    schoolCenter.value = null
  }
}

onMounted(() => {
  void load()
  void loadSchoolCenter()
  void loadTerm()
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.header-actions { display: flex; gap: 0.5rem; flex-wrap: wrap; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.row-actions { display: flex; gap: 0.25rem; }
/* Yer tutucu (İşletme adı, adres veya yetkili ara) kırpılmadan sığsın diye
   tarayıcının varsayılan girdi genişliği yerine sabit bir genişlik verilir. */
.search-input { width: 22rem; }
</style>
