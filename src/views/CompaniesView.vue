<template>
  <div class="page">
    <div class="page-header">
      <h1 class="page-title">{{ labels.company.title }}</h1>
      <Button :label="labels.common.add" icon="pi pi-plus" @click="openCreate" />
    </div>

    <Message severity="secondary" :closable="false">{{ labels.company.termNote }}</Message>

    <DataTable
      :value="companies"
      :loading="isLoading"
      paginator
      :rows="15"
      dataKey="id"
      v-model:filters="filters"
      :globalFilterFields="['name', 'addressText']"
      sortMode="single"
      stripedRows
    >
      <template #header>
        <InputText
          v-model="filters.global.value"
          :placeholder="labels.company.searchPlaceholder"
        />
      </template>

      <template #empty>{{ labels.company.empty }}</template>

      <Column field="name" :header="labels.company.name" sortable />
      <Column field="addressText" :header="labels.company.address" />
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
import { onMounted, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import CompanyFormDialog from '../components/company/CompanyFormDialog.vue'
import LocationPickerMap from '../components/map/LocationPickerMap.vue'
import { companiesApi } from '../api/companies'
import { settingsApi } from '../api/settings'
import { labels } from '../i18n/labels'
import { roundTripDistanceKm } from '../types/models'
import type { Company, GeocodeStatus, LatLng, NewCompany } from '../types/models'

const toast = useToast()
const confirm = useConfirm()

const companies = ref<Company[]>([])
const isLoading = ref(false)
const isDialogOpen = ref(false)
const selected = ref<Company | null>(null)
const filters = ref({ global: { value: null as string | null, matchMode: 'contains' } })

const isLocationDialogOpen = ref(false)
const locationTarget = ref<Company | null>(null)
const editedLocation = ref<LatLng | null>(null)
// Konumu olmayan işletme için harita okul konumuna odaklanır.
const schoolCenter = ref<LatLng | null>(null)

function formatKm(value: number | null): string {
  return value === null ? '—' : value.toFixed(1)
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
        await companiesApi.remove(company.id)
        toast.add({ severity: 'success', summary: labels.common.deleted, life: 2500 })
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
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.row-actions { display: flex; gap: 0.25rem; }
</style>
