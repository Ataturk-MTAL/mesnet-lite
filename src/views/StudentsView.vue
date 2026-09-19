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
            <Button icon="pi pi-arrow-right-arrow-left" severity="secondary" outlined size="small"
                    :disabled="data.companyId === null || currentTerm === null"
                    :aria-label="labels.studentChange.title" v-tooltip.top="labels.studentChange.title"
                    @click="openChangeDialog(data)" />
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

    <StudentChangeDialog
      v-model:visible="isChangeDialogOpen"
      :student="changeSubject"
      :term="changeTerm"
      @saved="load"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import StudentFormDialog from '../components/student/StudentFormDialog.vue'
import StudentChangeDialog, { type StudentChangeSubject } from '../components/student/StudentChangeDialog.vue'
import { studentsApi } from '../api/students'
import { companiesApi } from '../api/companies'
import { listTermsWithDates } from '../api/terms'
import { labels } from '../i18n/labels'
import type { Company, NewStudent, Student, TermWithDates } from '../types/models'
import { activeTerm } from '../composables/useTerm'

const toast = useToast()
const confirm = useConfirm()

const students = ref<Student[]>([])
const companies = ref<Company[]>([])
const termsWithDates = ref<TermWithDates[]>([])
const isLoading = ref(false)
const isDialogOpen = ref(false)
const selected = ref<Student | null>(null)
const filters = ref({ global: { value: null as string | null, matchMode: 'contains' } })

const isChangeDialogOpen = ref(false)
const changeSubject = ref<StudentChangeSubject>({ id: 0, fullName: '', companyId: null, companyName: null })

// Nakil/ayrılış diyaloğunun tarih kuralları için aktif dönemin tarihleri.
const currentTerm = computed(() => termsWithDates.value.find((t) => t.term === activeTerm.value) ?? null)
// Diyalog `TermWithDates` zorunlu kılar; yalnızca `currentTerm` doluyken açılır,
// bu değer görünmez durumdayken kullanılan zararsız bir yer tutucudur.
const changeTerm = computed<TermWithDates>(
  () =>
    currentTerm.value ?? {
      term: '',
      startDate: '',
      endDate: '',
      datesConfirmed: false,
      isPlanning: true,
      defaultAsOf: '',
      earliestAllowedDate: '',
    },
)

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
    // Üç liste birlikte yüklenir: öğrenci tablosu işletme adını göstermek için
    // işletme listesine, nakil/ayrılış diyaloğu da dönem tarihlerine ihtiyaç duyar.
    const [studentRows, companyRows, termRows] = await Promise.all([
      studentsApi.list(),
      companiesApi.list(),
      listTermsWithDates(),
    ])
    students.value = studentRows
    companies.value = companyRows
    termsWithDates.value = termRows
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

function openChangeDialog(student: Student): void {
  // Düğme zaten `currentTerm === null` iken devre dışıdır; bu yalnız savunma amaçlıdır.
  if (currentTerm.value === null) return
  changeSubject.value = {
    id: student.id,
    fullName: `${student.firstName} ${student.lastName}`,
    companyId: student.companyId,
    companyName: companyName(student.companyId),
  }
  isChangeDialogOpen.value = true
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
