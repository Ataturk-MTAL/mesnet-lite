<template>
  <div class="page">
    <div class="page-header">
      <div class="title-group">
        <h1 class="page-title">{{ labels.availability.title }}</h1>
        <Tag v-if="board?.term" :value="board.term" severity="secondary" icon="pi pi-calendar" />
      </div>
    </div>

    <Message severity="secondary" :closable="false">{{ labels.availability.subtitle }}</Message>

    <Message
      v-for="(warning, index) in board?.warnings ?? []"
      :key="index"
      severity="warn"
      :closable="false"
    >
      {{ warning }}
    </Message>

    <!-- Öğretmen boş saatleri (solda) ve program geçmişi (sağda; dar ekranda altta) -->
    <div class="teacher-layout" :class="{ 'teacher-layout--with-history': showHistory }">
    <Card class="teacher-card">
      <template #title>{{ labels.availability.teacherSection }}</template>
      <template #content>
        <div v-if="(board?.teachers.length ?? 0) === 0" class="empty">
          {{ labels.availability.noTeachers }}
        </div>

        <template v-else>
          <div class="toolbar">
            <Select
              v-model="selectedTeacherId"
              :options="board?.teachers ?? []"
              optionLabel="teacherName"
              optionValue="teacherId"
              :placeholder="labels.availability.selectTeacher"
              class="teacher-select"
              filter
            />
            <Tag
              v-if="selectedTeacher"
              :value="`${labels.availability.freeCount}: ${draftSlots.size}`"
              severity="secondary"
            />
            <div class="spacer" />
            <Button
              :label="labels.availability.selectAll"
              severity="secondary"
              outlined
              size="small"
              :disabled="!selectedTeacher"
              @click="selectAllSlots"
            />
            <Button
              :label="labels.availability.clearAll"
              severity="secondary"
              outlined
              size="small"
              :disabled="!selectedTeacher"
              @click="clearAllSlots"
            />
            <Button
              :label="isCorrecting ? labels.availability.saveCorrection : labels.availability.saveTeacher"
              icon="pi pi-check"
              :disabled="!canSaveTeacher"
              :loading="isSavingTeacher"
              data-testid="availability-save-teacher-button"
              @click="saveTeacher"
            />
          </div>

          <Message v-if="isCorrecting" class="correcting-message" severity="info" :closable="false" data-testid="availability-correcting-banner">
            <div class="correcting-banner">
              <div class="correcting-text">
                <strong>{{ labels.availability.correcting }}</strong>
                <span>{{ labels.availability.correctingHint }}</span>
              </div>
              <Button
                :label="labels.availability.cancelCorrection"
                :aria-label="labels.availability.cancelCorrection"
                size="small"
                severity="secondary"
                outlined
                data-testid="availability-cancel-correction-button"
                @click="cancelCorrection"
              />
            </div>
          </Message>

          <small class="hint">{{ labels.availability.toggleHint }}</small>

          <div v-if="selectedTeacher" class="grid-scroll">
            <table class="grid" @mouseleave="isPainting = false">
              <thead>
                <tr>
                  <th class="grid-hour-head">{{ labels.availability.hour }}</th>
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
                    :class="{ 'grid-cell--free': isFree(day, hour) }"
                    role="button"
                    tabindex="0"
                    :aria-pressed="isFree(day, hour)"
                    @mousedown="startPainting(day, hour)"
                    @mouseenter="paintOver(day, hour)"
                    @mouseup="isPainting = false"
                    @keydown.enter.prevent="toggleSlot(day, hour)"
                    @keydown.space.prevent="toggleSlot(day, hour)"
                  >
                    {{ isFree(day, hour) ? labels.availability.free : labels.availability.busy }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </template>
      </template>
    </Card>

    <TeacherScheduleHistory
      v-if="showHistory && term"
      :teacher-id="selectedTeacherId"
      :term="term.term"
      :refresh-token="historyRefreshToken"
      @edit="startCorrection"
      @changed="onHistoryChanged"
    />
    </div>

    <!-- Sınıfların işletme günleri -->
    <Card>
      <template #title>{{ labels.availability.classSection }}</template>
      <template #content>
        <div v-if="(board?.classes.length ?? 0) === 0" class="empty">
          {{ labels.availability.noClasses }}
        </div>

        <DataTable v-else :value="board?.classes ?? []" dataKey="grade" stripedRows>
          <Column field="grade" :header="labels.availability.grade" />
          <Column field="studentCount" :header="labels.availability.students" />
          <Column :header="labels.availability.workplaceDays">
            <template #body="{ data }">
              <SelectButton
                :model-value="classDayDraft[data.grade] ?? data.days"
                :options="dayOptions"
                optionLabel="label"
                optionValue="value"
                multiple
                @update:model-value="(value: number[]) => setClassDays(data.grade, value)"
              />
            </template>
          </Column>
          <Column>
            <template #body="{ data }">
              <Button
                :label="labels.availability.saveClass"
                icon="pi pi-check"
                size="small"
                :disabled="!isClassDirty(data)"
                @click="saveClass(data.grade)"
              />
            </template>
          </Column>
        </DataTable>
      </template>
    </Card>

    <!-- Dönem kopyalama -->
    <Card v-if="(board?.otherTerms.length ?? 0) > 0">
      <template #title>{{ labels.availability.copyTitle }}</template>
      <template #content>
        <div class="toolbar">
          <Select
            v-model="copySourceTerm"
            :options="board?.otherTerms ?? []"
            :placeholder="labels.availability.copySelect"
            class="teacher-select"
          />
          <Button
            :label="labels.availability.copyButton"
            icon="pi pi-copy"
            severity="secondary"
            outlined
            :disabled="!copySourceTerm"
            @click="copyFromTerm"
          />
        </div>
        <small class="hint">{{ labels.availability.copyHint }}</small>
      </template>
    </Card>

    <ChangeDetailsDialog
      v-if="term && !term.isPlanning"
      :visible="isDetailsOpen"
      :term="term"
      :title="labels.history.changeDetailsTitle"
      :effective-date="correctingEntry?.effectiveDate ?? null"
      :reason="correctingEntry?.reason ?? ''"
      @confirm="onDetailsConfirm"
      @cancel="isDetailsOpen = false"
    />

    <ImpactDialog :change="scheduleChange" @cancel="onScheduleChangeCancel" />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { availabilityApi } from '../api/availability'
import type { AvailabilityBoard, ClassDays, SlotInput } from '../api/availability'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'
import { listTermsWithDates } from '../api/terms'
import { useChange } from '../composables/useChange'
import ChangeDetailsDialog from '../components/history/ChangeDetailsDialog.vue'
import ImpactDialog from '../components/history/ImpactDialog.vue'
import TeacherScheduleHistory from '../components/history/TeacherScheduleHistory.vue'
import type { ChangeCommand, ChangeRequest, HistoryChangeSetEntry, TermWithDates } from '../types/models'

const DAYS = [1, 2, 3, 4, 5] as const

const toast = useToast()

const board = ref<AvailabilityBoard | null>(null)
const selectedTeacherId = ref<number | null>(null)
const isSavingTeacher = ref(false)
const copySourceTerm = ref<string | null>(null)

/** Aktif dönemin tarihleri; dönem başladıysa program kaydı tarihçe üzerinden gider. */
const term = ref<TermWithDates | null>(null)
const scheduleChange = useChange()
/** "Boş Saatleri Kaydet"e basılınca tarih ve gerekçeyi soran pencere. */
const isDetailsOpen = ref(false)

/** Düzeltilmekte olan son değişiklik; `null` ise normal kayıt modundayız. */
const correctingEntry = ref<HistoryChangeSetEntry | null>(null)
/** Geçmiş panelini yeniletmek için artırılır. */
const historyRefreshToken = ref(0)

/** Izgarada düzenlenen boş saatler; kaydedilene kadar sunucuya gitmez. */
const draftSlots = ref<Set<string>>(new Set())

/** Sürükleyerek boyama: basılı tutulurken geçilen hücreler aynı değere ayarlanır. */
const isPainting = ref(false)
const paintValue = ref(true)

/** Sınıf günleri düzenlemeleri. */
const classDayDraft = reactive<Record<string, number[]>>({})

const dayOptions = DAYS.map((day) => ({ value: day, label: labels.allocation.days[day] }))

const selectedTeacher = computed(
  () => board.value?.teachers.find((t) => t.teacherId === selectedTeacherId.value) ?? null,
)

const gridHours = computed(() => {
  const start = board.value?.dayStartHour ?? 8
  const end = board.value?.dayEndHour ?? 17
  return Array.from({ length: Math.max(0, end - start) }, (_, index) => start + index)
})

const isCorrecting = computed(() => correctingEntry.value !== null)

/** Dönem başlamadıysa hiçbir değişiklik tarihçeye yazılmaz; panel gösterilmez. */
const showHistory = computed(() => term.value !== null && !term.value.isPlanning)

/** Kaydedilmemiş değişiklik var mı? */
const teacherDirty = computed(() => {
  const saved = new Set(selectedTeacher.value?.freeSlots ?? [])
  if (saved.size !== draftSlots.value.size) return true
  for (const entry of draftSlots.value) {
    if (!saved.has(entry)) return true
  }
  return false
})

/**
 * Izgara değiştiyse ya da düzeltme modundaysa kaydedilebilir. Yürürlük tarihi ve
 * gerekçe düğmeye bağlı değildir; dönem başladıysa kaydederken pencerede sorulur.
 */
const canSaveTeacher = computed(
  () => selectedTeacher.value !== null && term.value !== null && (teacherDirty.value || isCorrecting.value),
)

function slotKey(day: number, hour: number): string {
  return `${day}-${hour}`
}

function isFree(day: number, hour: number): boolean {
  return draftSlots.value.has(slotKey(day, hour))
}

function setSlot(day: number, hour: number, value: boolean): void {
  // Set mutasyonu reaktif değildir; yeni Set atanır.
  const next = new Set(draftSlots.value)
  if (value) {
    next.add(slotKey(day, hour))
  } else {
    next.delete(slotKey(day, hour))
  }
  draftSlots.value = next
}

function toggleSlot(day: number, hour: number): void {
  setSlot(day, hour, !isFree(day, hour))
}

function startPainting(day: number, hour: number): void {
  // Boyama yönü ilk hücrenin TERSİ olur: dolu hücreden başlanırsa boşaltır.
  paintValue.value = !isFree(day, hour)
  isPainting.value = true
  setSlot(day, hour, paintValue.value)
}

function paintOver(day: number, hour: number): void {
  if (!isPainting.value) return
  setSlot(day, hour, paintValue.value)
}

function selectAllSlots(): void {
  const next = new Set<string>()
  for (const day of DAYS) {
    for (const hour of gridHours.value) {
      next.add(slotKey(day, hour))
    }
  }
  draftSlots.value = next
}

function clearAllSlots(): void {
  draftSlots.value = new Set()
}

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

function syncDraftFromBoard(): void {
  draftSlots.value = new Set(selectedTeacher.value?.freeSlots ?? [])
}

function applyBoard(next: AvailabilityBoard): void {
  board.value = next
  if (selectedTeacherId.value === null && next.teachers.length > 0) {
    selectedTeacherId.value = next.teachers[0].teacherId
  }
  syncDraftFromBoard()
  // Kaydedilmiş sınıf taslakları temizlenir; kaydedilmemişler korunur.
  for (const cls of next.classes) {
    const draft = classDayDraft[cls.grade]
    if (draft && draft.length === cls.days.length && draft.every((d) => cls.days.includes(d))) {
      delete classDayDraft[cls.grade]
    }
  }
}

async function load(): Promise<void> {
  try {
    applyBoard(await availabilityApi.get())
  } catch (error: unknown) {
    showError(error)
  }
}

function draftSlotsAsInput(): SlotInput[] {
  return [...draftSlots.value].map((entry) => {
    const [day, hour] = entry.split('-').map(Number)
    return { dayOfWeek: day, hour }
  })
}

/** Planlama evresinde: doğrudan kaydeder, tarihçeye yazmaz. */
async function saveTeacherDirectly(teacherId: number): Promise<void> {
  isSavingTeacher.value = true
  try {
    applyBoard(await availabilityApi.saveTeacher(teacherId, draftSlotsAsInput()))
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSavingTeacher.value = false
  }
}

/** Pencereden gelen tarih ve gerekçe. */
interface ChangeDetails {
  effectiveDate: string | null
  reason: string
}

/** Dönem başladıysa: `setTeacherSchedule` (düzeltmede `correct`) önizlenir, etki penceresinde onaylanır. */
async function saveTeacherViaChange(teacherId: number, details: ChangeDetails): Promise<void> {
  if (!term.value) return
  const setSchedule: ChangeCommand = { type: 'setTeacherSchedule', teacherId, slots: draftSlotsAsInput() }
  // Düzeltme modunda aynı yeni program, son değişikliği değiştiren `correct` komutuna sarılır.
  const command: ChangeCommand = correctingEntry.value
    ? { type: 'correct', changeSetId: correctingEntry.value.changeSetId, replacement: setSchedule }
    : setSchedule
  const request: ChangeRequest = {
    term: term.value.term,
    effectiveDate: details.effectiveDate,
    documentDate: null,
    reason: details.reason,
    command,
  }
  await scheduleChange.preview(request)
}

/** Planlamada doğrudan kaydeder; dönem başladıysa önce tarih ve gerekçe sorulur. */
async function saveTeacher(): Promise<void> {
  const teacherId = selectedTeacherId.value
  if (teacherId === null || !canSaveTeacher.value) return

  if (term.value?.isPlanning) {
    await saveTeacherDirectly(teacherId)
  } else {
    isDetailsOpen.value = true
  }
}

async function onDetailsConfirm(details: ChangeDetails): Promise<void> {
  isDetailsOpen.value = false
  const teacherId = selectedTeacherId.value
  if (teacherId === null) return
  await saveTeacherViaChange(teacherId, details)
}

function onScheduleChangeCancel(): void {
  // Etki penceresi kapatıldı; ızgara taslağı korunur, kullanıcı tekrar kaydetmeyi deneyebilir.
}

/** Düzenle: tarih ve gerekçe o değişikliğin değerleriyle dolar; ızgara zaten son durumu gösterir. */
function startCorrection(entry: HistoryChangeSetEntry): void {
  correctingEntry.value = entry
}

function cancelCorrection(): void {
  correctingEntry.value = null
}

/** "Geçmişten sil" kaydedildi: sunucudaki önceki duruma dönmek için ızgara yenilenir. */
async function onHistoryChanged(): Promise<void> {
  cancelCorrection()
  await load()
}

function setClassDays(grade: string, days: number[]): void {
  classDayDraft[grade] = [...days].sort((a, b) => a - b)
}

function isClassDirty(cls: ClassDays): boolean {
  const draft = classDayDraft[cls.grade]
  if (!draft) return false
  if (draft.length !== cls.days.length) return true
  return !draft.every((day) => cls.days.includes(day))
}

async function saveClass(grade: string): Promise<void> {
  try {
    applyBoard(await availabilityApi.saveClassDays(grade, classDayDraft[grade] ?? []))
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  }
}

async function copyFromTerm(): Promise<void> {
  const source = copySourceTerm.value
  if (!source) return

  try {
    const outcome = await availabilityApi.copyFromTerm(source)
    applyBoard(outcome.board)

    // Kopyalama kısmi olabilir: hedefte kaydı olan tür atlanır.
    const both = outcome.availabilityCopied && outcome.classDaysCopied
    const none = !outcome.availabilityCopied && !outcome.classDaysCopied
    const summary = both
      ? labels.availability.copiedBoth
      : none
        ? labels.availability.copiedNone
        : labels.availability.copiedPartial
    toast.add({ severity: none ? 'warn' : 'success', summary, life: 5000 })
  } catch (error: unknown) {
    showError(error)
  }
}

/** Aktif dönemin tarihlerini yükler; bulunamazsa (henüz tanımlanmamış dönem) `null` kalır. */
async function loadTerm(): Promise<void> {
  try {
    const allTerms = await listTermsWithDates()
    term.value = allTerms.find((t) => t.term === activeTerm.value) ?? null
  } catch (error: unknown) {
    showError(error)
  }
}

// Program değişikliği kaydedilince ızgara tazelenir ve tarih/gerekçe sıfırlanır.
watch(
  () => scheduleChange.status.value,
  async (status) => {
    if (status !== 'committed') return
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
    cancelCorrection()
    scheduleChange.reset()
    historyRefreshToken.value += 1
    await load()
  },
)

// Öğretmen değişince ızgara o öğretmenin kayıtlı saatlerini gösterir.
watch(selectedTeacherId, () => {
  // Başka öğretmene geçilince düzeltme modu o öğretmene ait olmadığı için kapanır.
  if (isCorrecting.value) cancelCorrection()
  syncDraftFromBoard()
})

watch(activeTerm, () => {
  void load()
  void loadTerm()
})

onMounted(async () => {
  await load()
  await loadTerm()
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; }
.title-group { display: flex; align-items: center; gap: 0.75rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }

/* Geniş ekranda ızgara solda, geçmiş sağda; dar ekranda geçmiş ızgaranın altına yığılır. */
.teacher-layout { display: grid; grid-template-columns: minmax(0, 1fr); gap: 1rem; align-items: start; }
@media (min-width: 72rem) {
  .teacher-layout--with-history { grid-template-columns: minmax(0, 1fr) 22rem; }
}
/* Message metni içeriği kadar dar kalır; Vazgeç sağa yaslansın diye satırı doldurur. */
.correcting-message { margin-top: 0.75rem; }
.correcting-message :deep(.p-message-text) { flex: 1; }
.correcting-banner { display: flex; align-items: center; justify-content: space-between; gap: 1rem; flex-wrap: wrap; width: 100%; }
.correcting-text { display: flex; flex-direction: column; gap: 0.125rem; }

.toolbar { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
.spacer { flex: 1; }
.teacher-select { min-width: 16rem; }
.hint { display: block; margin-top: 0.5rem; color: var(--p-text-muted-color); font-size: 0.75rem; }
.empty { color: var(--p-text-muted-color); }

.grid-scroll { overflow-x: auto; margin-top: 1rem; }
.grid { width: 100%; border-collapse: collapse; user-select: none; }
.grid th,
.grid td {
  border: 1px solid var(--p-content-border-color);
  padding: 0.375rem 0.5rem;
  font-size: 0.8125rem;
  text-align: center;
}
.grid-hour-head,
.grid-hour { width: 3.5rem; color: var(--p-text-muted-color); font-weight: 500; }
.grid-cell { cursor: pointer; color: var(--p-text-muted-color); min-width: 6rem; }
.grid-cell:hover { background: var(--p-content-hover-background); }
.grid-cell--free {
  background: var(--p-highlight-background);
  color: var(--p-highlight-color);
  font-weight: 500;
}
</style>
