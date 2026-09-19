<template>
  <div class="page">
    <div class="page-header">
      <h1 class="page-title">{{ labels.teacher.title }}</h1>
      <Button :label="labels.common.add" icon="pi pi-plus" @click="openCreate" />
    </div>

    <Message v-if="teachers.length > 0" severity="info" :closable="false">
      {{ labels.teacher.totalCapacity }}: <strong>{{ totalCapacity }}</strong>
      {{ labels.settings.hoursPerWeek }} —
      {{ activeCount }} {{ labels.teacher.activeTeachers }}
    </Message>

    <DataTable
      :value="teachers"
      :loading="isLoading"
      paginator
      :rows="20"
      dataKey="id"
      sortMode="single"
      stripedRows
    >
      <template #empty>{{ labels.teacher.empty }}</template>

      <Column field="lastName" :header="labels.teacher.lastName" sortable />
      <Column field="firstName" :header="labels.teacher.firstName" sortable />
      <Column field="registryNo" :header="labels.teacher.registryNo" />

      <Column :header="labels.teacher.branches">
        <template #body="{ data }">
          <div class="branch-tags">
            <Tag
              v-for="branch in parseBranches(data.branches)"
              :key="branch"
              :value="branch"
              severity="secondary"
            />
            <span v-if="parseBranches(data.branches).length === 0" class="muted">—</span>
          </div>
        </template>
      </Column>

      <Column :header="labels.teacher.chiefType">
        <template #body="{ data }">
          {{ labels.chiefType[data.chiefType as ChiefType] }}
          <span v-if="data.chiefHours > 0" class="muted"> ({{ data.chiefHours }} sa.)</span>
        </template>
      </Column>

      <Column :header="labels.teacher.capacity" sortable field="capacity">
        <template #body="{ data }">
          <div class="capacity">
            <strong>{{ data.capacity }}</strong>
            <span class="muted">/ {{ data.statutoryCap }}</span>
            <ProgressBar
              :value="capacityPercent(data)"
              :showValue="false"
              class="capacity-bar"
            />
          </div>
        </template>
      </Column>

      <Column :header="labels.teacher.isActive">
        <template #body="{ data }">
          <Tag
            :value="data.isActive === 1 ? labels.common.yes : labels.common.no"
            :severity="data.isActive === 1 ? 'success' : 'secondary'"
          />
        </template>
      </Column>

      <Column>
        <template #body="{ data }">
          <div class="row-actions">
            <Button icon="pi pi-pencil" severity="secondary" outlined size="small"
                    :aria-label="labels.common.edit" v-tooltip.top="labels.common.edit"
                    @click="openEdit(data)" />
            <Button icon="pi pi-sliders-h" severity="secondary" outlined size="small"
                    :aria-label="labels.teacherLoadChange.title" v-tooltip.top="labels.teacherLoadChange.title"
                    :disabled="!term"
                    @click="openLoadChange(data)" />
            <Button icon="pi pi-trash" severity="danger" outlined size="small"
                    :aria-label="labels.common.delete" v-tooltip.top="labels.common.delete"
                    @click="confirmRemove(data)" />
          </div>
        </template>
      </Column>
    </DataTable>

    <TeacherFormDialog
      v-model:visible="isDialogOpen"
      :teacher="selected"
      :known-branches="knownBranches"
      @save="handleSave"
    />

    <TeacherLoadDialog
      v-if="term"
      v-model:visible="isLoadDialogOpen"
      :teacher="loadSelected"
      :term="term"
      @saved="handleLoadSaved"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import TeacherFormDialog from '../components/teacher/TeacherFormDialog.vue'
import TeacherLoadDialog from '../components/teacher/TeacherLoadDialog.vue'
import { teachersApi } from '../api/teachers'
import { studentsApi } from '../api/students'
import { listTermsWithDates } from '../api/terms'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'
import { parseBranches } from '../types/models'
import type { ChiefType, NewTeacher, TeacherWithCapacity, TermWithDates } from '../types/models'

const toast = useToast()
const confirm = useConfirm()

const teachers = ref<TeacherWithCapacity[]>([])
const knownBranches = ref<string[]>([])
const isLoading = ref(false)
const isDialogOpen = ref(false)
const selected = ref<TeacherWithCapacity | null>(null)

/** Aktif dönemin tarihleri; "Yük değişikliği" penceresi bu olmadan açılamaz. */
const term = ref<TermWithDates | null>(null)
const isLoadDialogOpen = ref(false)
const loadSelected = ref<TeacherWithCapacity | null>(null)

const activeCount = computed(() => teachers.value.filter((t) => t.isActive === 1).length)

// Yalnızca aktif öğretmenler dağıtıma girer; toplam kapasite onlardan hesaplanır.
const totalCapacity = computed(() =>
  teachers.value.filter((t) => t.isActive === 1).reduce((sum, t) => sum + t.capacity, 0),
)

function capacityPercent(teacher: TeacherWithCapacity): number {
  if (teacher.statutoryCap <= 0) return 0
  return Math.round((teacher.capacity / teacher.statutoryCap) * 100)
}

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

async function load(): Promise<void> {
  isLoading.value = true
  try {
    teachers.value = await teachersApi.listWithCapacity()
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

/** Dal önerileri öğrenci kayıtlarından ve mevcut öğretmenlerden toplanır. */
async function loadKnownBranches(): Promise<void> {
  try {
    const students = await studentsApi.list()
    const fromStudents = students.map((s) => s.branch)
    const fromTeachers = teachers.value.flatMap((t) => parseBranches(t.branches))
    knownBranches.value = [...new Set([...fromStudents, ...fromTeachers])]
      .filter((b) => b.trim().length > 0)
      .sort((a, b) => a.localeCompare(b, 'tr'))
  } catch {
    // Öneri listesi yüklenemezse form yine de serbest metinle çalışır.
    knownBranches.value = []
  }
}

function openCreate(): void {
  selected.value = null
  isDialogOpen.value = true
}

function openEdit(teacher: TeacherWithCapacity): void {
  selected.value = teacher
  isDialogOpen.value = true
}

async function handleSave(input: NewTeacher): Promise<void> {
  try {
    if (selected.value) {
      await teachersApi.update(selected.value.id, input)
    } else {
      await teachersApi.create(input)
    }
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
    await load()
    await loadKnownBranches()
  } catch (error: unknown) {
    showError(error)
  }
}

/** Aktif dönemin tarihlerini yükler; "Yük değişikliği" penceresi buna göre açılır. */
async function loadTerm(): Promise<void> {
  try {
    const allTerms = await listTermsWithDates()
    term.value = allTerms.find((t) => t.term === activeTerm.value) ?? null
  } catch (error: unknown) {
    showError(error)
  }
}

function openLoadChange(teacher: TeacherWithCapacity): void {
  if (!term.value) return
  loadSelected.value = teacher
  isLoadDialogOpen.value = true
}

async function handleLoadSaved(): Promise<void> {
  toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
  await load()
}

function confirmRemove(teacher: TeacherWithCapacity): void {
  confirm.require({
    message: labels.teacher.deleteConfirm,
    header: `${teacher.firstName} ${teacher.lastName}`,
    acceptLabel: labels.common.yes,
    rejectLabel: labels.common.no,
    acceptProps: { severity: 'danger' },
    accept: async () => {
      try {
        await teachersApi.remove(teacher.id)
        toast.add({ severity: 'success', summary: labels.common.deleted, life: 2500 })
        await load()
      } catch (error: unknown) {
        showError(error)
      }
    },
  })
}

onMounted(async () => {
  await load()
  await loadKnownBranches()
  await loadTerm()
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.row-actions { display: flex; gap: 0.25rem; }
.branch-tags { display: flex; flex-wrap: wrap; gap: 0.25rem; }
.capacity { display: flex; align-items: center; gap: 0.5rem; min-width: 10rem; }
.capacity-bar { flex: 1; height: 0.5rem; }
.muted { color: var(--p-text-muted-color); }
</style>
