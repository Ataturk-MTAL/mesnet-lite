<template>
  <div class="page">
    <div class="page-header">
      <div class="title-group">
        <h1 class="page-title">{{ labels.student.title }}</h1>
        <Tag v-if="activeTerm" :value="activeTerm" severity="secondary" icon="pi pi-calendar" />
      </div>
      <Button :label="labels.common.add" icon="pi pi-plus" @click="openCreate" />
    </div>

    <Message severity="secondary" :closable="false">{{ labels.term.hint }}</Message>

    <DataTable
      :value="students"
      :loading="isLoading"
      paginator
      :rows="20"
      dataKey="id"
      v-model:filters="filters"
      :globalFilterFields="['firstName', 'lastName', 'studentNo', 'grade', 'branch']"
      sortMode="single"
      stripedRows
    >
      <template #header>
        <InputText v-model="filters.global.value" :placeholder="labels.student.searchPlaceholder" />
      </template>
      <template #empty>{{ labels.student.empty }}</template>

      <Column field="studentNo" :header="labels.student.studentNo" sortable>
        <template #body="{ data }">{{ data.studentNo ?? '—' }}</template>
      </Column>
      <Column field="firstName" :header="labels.student.firstName" sortable />
      <Column field="lastName" :header="labels.student.lastName" sortable />
      <Column field="grade" :header="labels.student.grade" sortable />
      <Column field="branch" :header="labels.student.branch" sortable />

      <Column :header="labels.student.company">
        <template #body="{ data }">
          <span v-if="companyName(data.companyId)">{{ companyName(data.companyId) }}</span>
          <Tag v-else :value="labels.student.noCompany" severity="warn" />
        </template>
      </Column>

      <Column>
        <template #body="{ data }">
          <div class="row-actions">
            <Button icon="pi pi-pencil" severity="secondary" outlined size="small"
                    :aria-label="labels.common.edit" v-tooltip.top="labels.common.edit"
                    @click="openEdit(data)" />
            <Button icon="pi pi-trash" severity="danger" outlined size="small"
                    :aria-label="labels.common.delete" v-tooltip.top="labels.common.delete"
                    @click="confirmRemove(data)" />
          </div>
        </template>
      </Column>
    </DataTable>

    <StudentFormDialog
      v-model:visible="isDialogOpen"
      :student="selected"
      :companies="companies"
      @save="handleSave"
    />
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import StudentFormDialog from '../components/student/StudentFormDialog.vue'
import { studentsApi } from '../api/students'
import { companiesApi } from '../api/companies'
import { labels } from '../i18n/labels'
import type { Company, NewStudent, Student } from '../types/models'
import { activeTerm } from '../composables/useTerm'

const toast = useToast()
const confirm = useConfirm()

const students = ref<Student[]>([])
const companies = ref<Company[]>([])
const isLoading = ref(false)
const isDialogOpen = ref(false)
const selected = ref<Student | null>(null)
const filters = ref({ global: { value: null as string | null, matchMode: 'contains' } })

function companyName(companyId: number | null): string | null {
  if (companyId === null) return null
  return companies.value.find((c) => c.id === companyId)?.name ?? null
}

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

async function load(): Promise<void> {
  isLoading.value = true
  try {
    // İki liste birlikte yüklenir; öğrenci tablosu işletme adını göstermek için
    // işletme listesine ihtiyaç duyar.
    const [studentRows, companyRows] = await Promise.all([
      studentsApi.list(),
      companiesApi.list(),
    ])
    students.value = studentRows
    companies.value = companyRows
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

function openEdit(student: Student): void {
  selected.value = student
  isDialogOpen.value = true
}

async function handleSave(input: NewStudent): Promise<void> {
  try {
    if (selected.value) {
      await studentsApi.update(selected.value.id, input)
    } else {
      await studentsApi.create(input)
    }
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
    await load()
  } catch (error: unknown) {
    showError(error)
  }
}

function confirmRemove(student: Student): void {
  confirm.require({
    message: labels.common.deleteConfirm,
    header: `${student.firstName} ${student.lastName}`,
    acceptLabel: labels.common.yes,
    rejectLabel: labels.common.no,
    acceptProps: { severity: 'danger' },
    accept: async () => {
      try {
        await studentsApi.remove(student.id)
        toast.add({ severity: 'success', summary: labels.common.deleted, life: 2500 })
        await load()
      } catch (error: unknown) {
        showError(error)
      }
    },
  })
}

// Üst çubuktan dönem değişince liste yeniden yüklenir.
watch(activeTerm, load)

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; }
.title-group { display: flex; align-items: center; gap: 0.75rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.row-actions { display: flex; gap: 0.25rem; }
</style>
