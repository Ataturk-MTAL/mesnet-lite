<template>
  <div class="page">
    <div class="page-header">
      <div class="title-group">
        <h1 class="page-title">{{ labels.allocation.title }}</h1>
        <Tag v-if="board?.term" :value="board.term" severity="secondary" icon="pi pi-calendar" />
      </div>
      <Button
        :label="labels.allocation.clearAll"
        icon="pi pi-trash"
        severity="danger"
        outlined
        :disabled="(board?.assignedCompanyCount ?? 0) === 0"
        @click="confirmClear"
      />
    </div>

    <Message severity="secondary" :closable="false">{{ labels.allocation.subtitle }}</Message>

    <Message
      v-for="(warning, index) in board?.warnings ?? []"
      :key="index"
      :severity="warning.includes('aşıyor') || warning.includes('aşıldı') ? 'error' : 'warn'"
      :closable="false"
    >
      {{ warning }}
    </Message>

    <!-- MESNET'teki dört sayaç -->
    <Card>
      <template #content>
        <div class="summary">
          <div class="summary-item">
            <div class="summary-value">{{ board?.poolHours ?? 0 }}</div>
            <div class="summary-label">{{ labels.allocation.poolHours }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value">{{ board?.assignedHours ?? 0 }}</div>
            <div class="summary-label">{{ labels.allocation.assignedHours }}</div>
          </div>
          <div class="summary-item">
            <div
              class="summary-value"
              :class="{ 'summary-value--over': (board?.remainingHours ?? 0) < 0 }"
            >
              {{ board?.remainingHours ?? 0 }}
            </div>
            <div class="summary-label">{{ labels.allocation.remainingHours }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value">
              {{ board?.assignedCompanyCount ?? 0 }} / {{ board?.totalCompanyCount ?? 0 }}
            </div>
            <div class="summary-label">
              {{ labels.allocation.assignedCount }}
              <span v-if="(board?.honoraryCount ?? 0) > 0">
                · {{ board?.honoraryCount }} {{ labels.allocation.honoraryNote }}
              </span>
            </div>
          </div>
        </div>
      </template>
    </Card>

    <div class="board">
      <!-- Sol: atanmamış işletme kartları -->
      <Card class="panel">
        <template #title>
          {{ labels.allocation.unassigned }} ({{ unassignedCompanies.length }})
        </template>
        <template #content>
          <InputText
            v-model="companySearch"
            :placeholder="labels.allocation.searchCompany"
            class="search"
          />

          <div v-if="filteredUnassigned.length === 0" class="empty">
            {{ labels.allocation.allAssigned }}
          </div>

          <div
            v-for="company in filteredUnassigned"
            :key="company.companyId"
            class="company-card"
            :class="{ 'company-card--dragging': draggedCompanyId === company.companyId }"
            draggable="true"
            tabindex="0"
            role="button"
            :aria-label="company.companyName"
            @dragstart="onDragStart($event, company.companyId)"
            @dragend="onDragEnd"
            @keydown.enter.prevent="toggleKeyboardSelection(company.companyId)"
            @keydown.space.prevent="toggleKeyboardSelection(company.companyId)"
          >
            <div class="company-name">{{ company.companyName }}</div>
            <div class="company-meta">
              <Tag v-if="company.isHonorary" :value="labels.hours.honorary" severity="info" />
              <Tag
                v-else-if="company.hoursMissing"
                :value="labels.allocation.hoursMissing"
                severity="warn"
              />
              <Tag v-else :value="`${company.awardedHours} saat`" severity="success" />

              <span class="muted">{{ company.studentCount }} öğrenci</span>
              <span v-if="company.oneWayDistanceKm !== null" class="muted">
                · {{ company.oneWayDistanceKm.toFixed(1) }} km
              </span>
            </div>
            <div v-if="company.branches.length > 0" class="company-branches">
              {{ company.branches.join(', ') }}
            </div>
            <div v-if="company.workplaceDays.length > 0" class="company-days">
              {{ company.workplaceDays.map((d) => labels.allocation.days[d]).join(', ') }}
            </div>
            <div v-else class="company-days company-days--missing">
              {{ labels.allocation.notWorkplaceDay }}
            </div>
          </div>
        </template>
      </Card>

      <!-- Sağ: öğretmen seçimi + haftalık ızgara -->
      <Card class="panel panel--grid">
        <template #title>{{ labels.allocation.weeklyGrid }}</template>
        <template #content>
          <Select
            v-model="selectedTeacherId"
            :options="board?.teachers ?? []"
            optionLabel="teacherName"
            optionValue="teacherId"
            :placeholder="labels.allocation.selectTeacher"
            class="teacher-select"
            filter
          />

          <div v-if="!selectedTeacher" class="empty">{{ labels.allocation.selectTeacher }}</div>

          <template v-else>
            <div class="teacher-stats">
              <Tag
                :value="`${labels.allocation.capacity}: ${selectedTeacher.assignedHours} / ${selectedTeacher.capacity}`"
                :severity="selectedTeacher.isOverCapacity ? 'danger' : 'secondary'"
              />
              <Tag
                :value="`${selectedTeacher.companyCount} ${labels.hours.company}`"
                severity="secondary"
              />
              <span v-if="selectedTeacher.branches.length > 0" class="muted">
                {{ selectedTeacher.branches.join(', ') }}
              </span>
            </div>

            <div class="grid-scroll">
              <table class="grid">
                <thead>
                  <tr>
                    <th class="grid-hour-head">{{ labels.allocation.hour }}</th>
                    <th v-for="day in DAYS" :key="day">{{ labels.allocation.days[day] }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="hour in gridHours" :key="hour">
                    <th class="grid-hour">{{ hour }}.</th>
                    <td
                      v-for="day in DAYS"
                      :key="`${day}-${hour}`"
                      class="grid-cell"
                      :class="cellClass(day, hour)"
                      @dragenter.prevent="onDragOver"
                      @dragover.prevent="onDragOver"
                      @drop.prevent.stop="onDrop($event, day, hour)"
                      @click="onCellClick(day, hour)"
                    >
                      <template v-if="cellCompany(day, hour)">
                        <span class="cell-name">{{ cellCompany(day, hour)?.companyName }}</span>
                        <Button
                          icon="pi pi-times"
                          severity="danger"
                          text
                          rounded
                          size="small"
                          :aria-label="labels.allocation.removeAssignment"
                          v-tooltip.top="labels.allocation.removeAssignment"
                          @click.stop="unassign(cellCompany(day, hour)!.companyId)"
                        />
                      </template>
                      <span v-else-if="isFree(day, hour)" class="cell-free">
                        {{ labels.allocation.free }}
                      </span>
                      <span v-else class="cell-unavailable">
                        {{ labels.allocation.unavailable }}
                      </span>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </template>
        </template>
      </Card>
    </div>

    <!-- Öğretmen özeti -->
    <Card>
      <template #title>{{ labels.allocation.teacherSummary }}</template>
      <template #content>
        <DataTable :value="board?.teachers ?? []" dataKey="teacherId" stripedRows>
          <Column field="teacherName" :header="labels.allocation.teacher" sortable />
          <Column field="companyCount" :header="labels.allocation.companyCount" sortable />
          <Column :header="labels.allocation.assignedHours" sortable field="assignedHours">
            <template #body="{ data }">
              <span :class="{ over: data.isOverCapacity }">
                {{ data.assignedHours }} / {{ data.capacity }}
              </span>
            </template>
          </Column>
          <Column v-for="day in DAYS" :key="day" :header="labels.allocation.days[day].slice(0, 3)">
            <template #body="{ data }">
              <span :class="{ over: data.daysOverCap.includes(day) }">
                {{ data.hoursPerDay[String(day)] ?? 0 }}
              </span>
            </template>
          </Column>
        </DataTable>
      </template>
    </Card>

    <!-- Zorlama diyaloğu -->
    <Dialog
      v-model:visible="isForceDialogOpen"
      modal
      :header="labels.allocation.forceTitle"
      :style="{ width: '30rem' }"
    >
      <p>{{ labels.allocation.forceQuestion }}</p>
      <ul class="force-reasons">
        <li v-for="(reason, index) in pendingViolations" :key="index">{{ reason }}</li>
      </ul>
      <div class="field">
        <label for="force-reason">{{ labels.allocation.forceReason }}</label>
        <Textarea id="force-reason" v-model="forceReason" rows="2" autofocus />
        <small class="muted">{{ labels.allocation.forceReasonHint }}</small>
      </div>
      <template #footer>
        <Button :label="labels.common.cancel" severity="secondary" outlined @click="cancelForce" />
        <Button
          :label="labels.allocation.forceConfirm"
          severity="warn"
          :disabled="forceReason.trim().length === 0"
          @click="confirmForce"
        />
      </template>
    </Dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import { assignmentsApi } from '../api/assignments'
import type { AssignmentBoard, BoardCompany, NewAssignment } from '../api/assignments'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'

const DAYS = [1, 2, 3, 4, 5] as const

const toast = useToast()
const confirm = useConfirm()

const board = ref<AssignmentBoard | null>(null)
const selectedTeacherId = ref<number | null>(null)
const companySearch = ref('')
const draggedCompanyId = ref<number | null>(null)

// Zorlama diyaloğu durumu
const isForceDialogOpen = ref(false)
const forceReason = ref('')
const pendingPlacement = ref<{ companyId: number; day: number; hour: number } | null>(null)
const pendingViolations = ref<string[]>([])

const selectedTeacher = computed(
  () => board.value?.teachers.find((t) => t.teacherId === selectedTeacherId.value) ?? null,
)

/** Izgara satırları ayarlardaki gün aralığından gelir. */
const gridHours = computed(() => {
  const start = board.value?.dayStartHour ?? 8
  const end = board.value?.dayEndHour ?? 17
  return Array.from({ length: Math.max(0, end - start) }, (_, index) => start + index)
})

const unassignedCompanies = computed(
  () => board.value?.companies.filter((c) => c.assignedTeacherId === null) ?? [],
)

const filteredUnassigned = computed(() => {
  const query = companySearch.value.trim().toLocaleLowerCase('tr')
  if (query.length === 0) return unassignedCompanies.value
  return unassignedCompanies.value.filter(
    (company) =>
      company.companyName.toLocaleLowerCase('tr').includes(query) ||
      company.addressText.toLocaleLowerCase('tr').includes(query),
  )
})

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

function isFree(day: number, hour: number): boolean {
  return selectedTeacher.value?.freeSlots.includes(`${day}-${hour}`) ?? false
}

function cellCompany(day: number, hour: number): BoardCompany | null {
  return (
    board.value?.companies.find(
      (c) =>
        c.assignedTeacherId === selectedTeacherId.value &&
        c.visitDay === day &&
        c.visitHour === hour,
    ) ?? null
  )
}

function cellClass(day: number, hour: number): Record<string, boolean> {
  const occupied = cellCompany(day, hour) !== null
  return {
    'grid-cell--busy': occupied,
    'grid-cell--free': !occupied && isFree(day, hour),
    'grid-cell--blocked': !occupied && !isFree(day, hour),
    'grid-cell--target': draggedCompanyId.value !== null && !occupied && isFree(day, hour),
  }
}

/**
 * WebKit (Tauri'nin macOS webview'i) `dataTransfer` boş bırakılırsa sürükleme
 * işlemini hiç başlatmaz. Chromium buna göz yumduğu için hata yalnızca
 * paketlenmiş uygulamada görünür. İşletme kimliğini yüke yazıyoruz.
 */
function onDragStart(event: DragEvent, companyId: number): void {
  draggedCompanyId.value = companyId
  if (event.dataTransfer) {
    event.dataTransfer.setData('text/plain', String(companyId))
    event.dataTransfer.effectAllowed = 'move'
  }
}

function onDragEnd(): void {
  draggedCompanyId.value = null
}

/** Hedef hücrede taşıma imlecini gösterir; önlenmezse bırakma gerçekleşmez. */
function onDragOver(event: DragEvent): void {
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = 'move'
  }
}

/** Klavye ile seçim: Enter kartı seçer, sonra hücrede tıklama bırakır. */
function toggleKeyboardSelection(companyId: number): void {
  draggedCompanyId.value = draggedCompanyId.value === companyId ? null : companyId
}

function onCellClick(day: number, hour: number): void {
  if (draggedCompanyId.value === null) return
  void place(draggedCompanyId.value, day, hour)
}

function onDrop(event: DragEvent, day: number, hour: number): void {
  // `dragend` bazı webview'larda `drop`tan önce tetiklenip ref'i temizler;
  // asıl kaynak sürükleme yüküdür, ref yalnızca yedek.
  const payload = Number(event.dataTransfer?.getData('text/plain'))
  const companyId = Number.isFinite(payload) && payload > 0 ? payload : draggedCompanyId.value
  if (companyId === null) return
  void place(companyId, day, hour)
}

/** Kural ihlallerini toplar. Boş dizi dönerse yerleşim temizdir. */
function collectViolations(company: BoardCompany, day: number, hour: number): string[] {
  const problems: string[] = []

  if (!isFree(day, hour)) {
    problems.push('Seçilen saat öğretmenin boş saatleri arasında değil.')
  }
  if (!company.workplaceDays.includes(day)) {
    problems.push(`Öğrenciler ${labels.allocation.days[day]} günü işletmede değil.`)
  }

  const teacher = selectedTeacher.value
  if (teacher) {
    const dayHours = teacher.hoursPerDay[String(day)] ?? 0
    if (dayHours + company.awardedHours > 8) {
      problems.push(
        `${labels.allocation.days[day]} günü toplam ${dayHours + company.awardedHours} saat olur; günlük sınır 8 (OÖKY MADDE 88).`,
      )
    }
    if (teacher.assignedHours + company.awardedHours > teacher.capacity) {
      problems.push(
        `Öğretmenin toplamı ${teacher.assignedHours + company.awardedHours} saate çıkar; kapasitesi ${teacher.capacity}.`,
      )
    }
  }

  return problems
}

async function place(companyId: number, day: number, hour: number): Promise<void> {
  const teacherId = selectedTeacherId.value
  if (teacherId === null) {
    toast.add({ severity: 'warn', summary: labels.allocation.selectTeacher, life: 4000 })
    return
  }

  const company = board.value?.companies.find((c) => c.companyId === companyId)
  if (!company) return

  const problems = collectViolations(company, day, hour)
  if (problems.length > 0) {
    // Kural dışı yerleşim engellenmez, gerekçe istenir ve kayda geçer.
    pendingPlacement.value = { companyId, day, hour }
    pendingViolations.value = problems
    forceReason.value = ''
    isForceDialogOpen.value = true
    return
  }

  await submit({
    teacherId,
    companyId,
    visitDay: day,
    visitHour: hour,
    isForced: false,
    forceReason: null,
  })
}

async function submit(input: NewAssignment): Promise<void> {
  try {
    board.value = await assignmentsApi.assign(input)
    draggedCompanyId.value = null
    toast.add({ severity: 'success', summary: labels.allocation.assigned, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  }
}

function cancelForce(): void {
  isForceDialogOpen.value = false
  pendingPlacement.value = null
  pendingViolations.value = []
}

async function confirmForce(): Promise<void> {
  const placement = pendingPlacement.value
  const teacherId = selectedTeacherId.value
  if (!placement || teacherId === null) return

  isForceDialogOpen.value = false
  await submit({
    teacherId,
    companyId: placement.companyId,
    visitDay: placement.day,
    visitHour: placement.hour,
    isForced: true,
    forceReason: forceReason.value.trim(),
  })
  pendingPlacement.value = null
  pendingViolations.value = []
}

async function unassign(companyId: number): Promise<void> {
  try {
    board.value = await assignmentsApi.unassign(companyId)
    toast.add({ severity: 'success', summary: labels.allocation.unassigned2, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  }
}

function confirmClear(): void {
  confirm.require({
    message: labels.allocation.clearConfirm,
    header: labels.allocation.clearAll,
    acceptLabel: labels.common.yes,
    rejectLabel: labels.common.no,
    acceptProps: { severity: 'danger' },
    accept: async () => {
      try {
        board.value = await assignmentsApi.clear()
        toast.add({ severity: 'success', summary: labels.allocation.cleared, life: 2500 })
      } catch (error: unknown) {
        showError(error)
      }
    },
  })
}

async function load(): Promise<void> {
  try {
    board.value = await assignmentsApi.get()
    // İlk öğretmen otomatik seçilsin ki ızgara boş görünmesin.
    if (selectedTeacherId.value === null && board.value.teachers.length > 0) {
      selectedTeacherId.value = board.value.teachers[0].teacherId
    }
  } catch (error: unknown) {
    showError(error)
  }
}

watch(activeTerm, load)

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.title-group { display: flex; align-items: center; gap: 0.75rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }

.summary { display: flex; flex-wrap: wrap; gap: 2.5rem; }
.summary-item { min-width: 9rem; }
.summary-value { font-size: 1.5rem; font-weight: 700; line-height: 1.1; }
.summary-value--over { color: var(--p-red-500); }
.summary-label { font-size: 0.8125rem; color: var(--p-text-muted-color); margin-top: 0.125rem; }

.board { display: flex; gap: 1rem; align-items: flex-start; flex-wrap: wrap; }
.panel { flex: 1; min-width: 20rem; }
.panel--grid { flex: 2; min-width: 28rem; }

.search { width: 100%; margin-bottom: 0.75rem; }
.empty { color: var(--p-text-muted-color); padding: 1rem 0; }

.company-card {
  border: 1px solid var(--p-content-border-color);
  border-radius: var(--p-content-border-radius);
  padding: 0.625rem 0.75rem;
  margin-bottom: 0.5rem;
  cursor: grab;
  background: var(--p-content-background);
  /* WebKit metin seçimini sürükleme sanır; elemanın kendisi sürüklenmeli. */
  -webkit-user-drag: element;
  user-select: none;
}
.company-card:active { cursor: grabbing; }
.company-card:hover { background: var(--p-content-hover-background); }
.company-card--dragging { outline: 2px solid var(--p-primary-color); }
.company-name { font-weight: 600; font-size: 0.9375rem; }
.company-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
  margin-top: 0.375rem;
}
.company-branches,
.company-days { font-size: 0.75rem; color: var(--p-text-muted-color); margin-top: 0.25rem; }
.company-days--missing { color: var(--p-orange-500); }

.teacher-select { width: 100%; margin-bottom: 0.75rem; }
.teacher-stats {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
  margin-bottom: 0.75rem;
}

.grid-scroll { overflow-x: auto; }
.grid { width: 100%; border-collapse: collapse; }
.grid th,
.grid td {
  border: 1px solid var(--p-content-border-color);
  padding: 0.375rem 0.5rem;
  font-size: 0.8125rem;
  text-align: center;
}
.grid-hour-head,
.grid-hour { width: 3.5rem; color: var(--p-text-muted-color); font-weight: 500; }
.grid-cell { height: 3rem; min-width: 8rem; }
.grid-cell--free { cursor: pointer; }
.grid-cell--blocked {
  background: var(--p-content-hover-background);
  color: var(--p-text-muted-color);
}
.grid-cell--busy {
  background: var(--p-highlight-background);
  color: var(--p-highlight-color);
  font-weight: 500;
}
.grid-cell--target { outline: 2px dashed var(--p-primary-color); }
.cell-free { color: var(--p-text-muted-color); }
.cell-unavailable { color: var(--p-text-muted-color); opacity: 0.6; }
.cell-name { margin-right: 0.25rem; }

.over { color: var(--p-red-500); font-weight: 600; }
.muted { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.force-reasons { margin: 0.5rem 0 1rem; padding-left: 1.25rem; color: var(--p-orange-500); }
.field { display: flex; flex-direction: column; gap: 0.375rem; }
</style>
