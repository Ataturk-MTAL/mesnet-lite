<template>
  <div class="page">
    <div class="page-header">
      <h1 class="page-title">{{ labels.company.title }}</h1>
      <Button :label="labels.common.add" @click="openCreate" />
    </div>

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
              :label="labels.common.edit"
              severity="secondary"
              text
              size="small"
              @click="openEdit(data)"
            />
            <Button
              :label="labels.common.delete"
              severity="danger"
              text
              size="small"
              @click="confirmRemove(data)"
            />
          </div>
        </template>
      </Column>
    </DataTable>

    <CompanyFormDialog v-model:visible="isDialogOpen" :company="selected" @save="handleSave" />
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import CompanyFormDialog from '../components/company/CompanyFormDialog.vue'
import { companiesApi } from '../api/companies'
import { labels } from '../i18n/labels'
import { roundTripDistanceKm } from '../types/models'
import type { Company, GeocodeStatus, NewCompany } from '../types/models'

const toast = useToast()
const confirm = useConfirm()

const companies = ref<Company[]>([])
const isLoading = ref(false)
const isDialogOpen = ref(false)
const selected = ref<Company | null>(null)
const filters = ref({ global: { value: null as string | null, matchMode: 'contains' } })

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

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.row-actions { display: flex; gap: 0.25rem; }
</style>
