<template>
  <Dialog
    :visible="visible"
    @update:visible="onVisibleChange"
    modal
    :header="labels.teacherLoadChange.title"
    :style="{ width: '32rem' }"
  >
    <div class="form-grid" v-if="teacher">
      <p class="teacher-name">{{ teacher.firstName }} {{ teacher.lastName }}</p>

      <div class="field-row">
        <div class="field">
          <label for="teacher-load-base">{{ labels.teacherLoadChange.baseHours }}</label>
          <InputNumber id="teacher-load-base" v-model="form.baseHours" :min="0" :max="40" />
        </div>
        <div class="field">
          <label for="teacher-load-max-extra">{{ labels.teacherLoadChange.maxExtraHours }}</label>
          <InputNumber id="teacher-load-max-extra" v-model="form.maxExtraHours" :min="0" :max="60" />
        </div>
      </div>

      <div class="field-row">
        <div class="field">
          <label for="teacher-load-other">{{ labels.teacherLoadChange.otherExtraHours }}</label>
          <InputNumber id="teacher-load-other" v-model="form.otherExtraHours" :min="0" :max="40" />
        </div>
        <div class="field">
          <label for="teacher-load-chief">{{ labels.teacherLoadChange.chiefType }}</label>
          <Select
            id="teacher-load-chief"
            v-model="form.chiefType"
            :options="chiefOptions"
            optionLabel="label"
            optionValue="value"
          />
        </div>
      </div>

      <div class="field">
        <label for="teacher-load-employment">{{ labels.teacherLoadChange.employmentType }}</label>
        <Select
          id="teacher-load-employment"
          v-model="form.employmentType"
          :options="employmentOptions"
          optionLabel="label"
          optionValue="value"
        />
      </div>

      <Message v-if="capacityWarning" severity="warn" :closable="false">{{ capacityWarning }}</Message>

      <EffectiveDateField v-model="effectiveDate" :label="labels.effectiveDateField.generic" :term="term" />

      <div class="field">
        <label for="teacher-load-reason">{{ labels.history.reason }}</label>
        <Textarea
          id="teacher-load-reason"
          v-model="reason"
          rows="2"
          :placeholder="labels.history.reasonPlaceholder"
          data-testid="teacher-load-reason"
        />
        <small v-if="isReasonEmpty" class="field-error">{{ labels.history.reasonRequired }}</small>
      </div>
    </div>

    <template #footer>
      <Button :label="labels.common.cancel" severity="secondary" outlined @click="close" />
      <Button
        :label="labels.common.save"
        :disabled="!isValid"
        data-testid="teacher-load-save-button"
        @click="startPreview"
      />
    </template>
  </Dialog>

  <ImpactDialog :change="change" @cancel="onImpactCancel" />
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { labels } from '../../i18n/labels'
import { useChange } from '../../composables/useChange'
import EffectiveDateField from '../history/EffectiveDateField.vue'
import ImpactDialog from '../history/ImpactDialog.vue'
import type { ChangeRequest, ChiefType, EmploymentType, TeacherWithCapacity, TermWithDates } from '../../types/models'

const props = defineProps<{
  visible: boolean
  /** `null` iken pencere gizlidir; satır seçimi TeachersView tarafında yapılır. */
  teacher: TeacherWithCapacity | null
  term: TermWithDates
}>()

const emit = defineEmits<{ 'update:visible': [value: boolean]; saved: [] }>()

interface TeacherLoadFormState {
  baseHours: number
  maxExtraHours: number
  otherExtraHours: number
  chiefType: ChiefType
  employmentType: EmploymentType
}

function loadFormFromTeacher(teacher: TeacherWithCapacity): TeacherLoadFormState {
  return {
    baseHours: teacher.baseHours,
    maxExtraHours: teacher.maxExtraHours,
    otherExtraHours: teacher.otherExtraHours,
    chiefType: teacher.chiefType,
    employmentType: teacher.employmentType,
  }
}

function emptyForm(): TeacherLoadFormState {
  return { baseHours: 0, maxExtraHours: 0, otherExtraHours: 0, chiefType: 'none', employmentType: 'tenured' }
}

const form = reactive<TeacherLoadFormState>(emptyForm())
const effectiveDate = ref<string | null>(null)
const reason = ref('')
const change = useChange()

const employmentOptions = [
  { value: 'tenured' as const, label: labels.employmentType.tenured },
  { value: 'contracted' as const, label: labels.employmentType.contracted },
]

const chiefOptions = [
  { value: 'none' as const, label: labels.chiefType.none },
  { value: 'workshop_lab' as const, label: labels.chiefType.workshop_lab },
  { value: 'department' as const, label: labels.chiefType.department },
]

/** MADDE 6/4 — şeflik saati, azami ek ders tavanının içinden düşer. */
const chiefHoursByType: Record<ChiefType, number> = { none: 0, workshop_lab: 6, department: 10 }

const capacityWarning = computed<string | null>(() =>
  form.maxExtraHours < chiefHoursByType[form.chiefType] ? labels.teacher.capacityWarning : null,
)

const isReasonEmpty = computed(() => reason.value.trim().length === 0)
const isDateMissing = computed(() => !props.term.isPlanning && effectiveDate.value === null)

const isValid = computed(
  () =>
    props.teacher !== null &&
    capacityWarning.value === null &&
    !isReasonEmpty.value &&
    !isDateMissing.value,
)

function buildRequest(teacher: TeacherWithCapacity): ChangeRequest {
  return {
    term: props.term.term,
    effectiveDate: props.term.isPlanning ? null : effectiveDate.value,
    documentDate: null,
    reason: reason.value.trim(),
    command: {
      type: 'setTeacherLoad',
      teacherId: teacher.id,
      load: {
        baseHours: form.baseHours,
        maxExtraHours: form.maxExtraHours,
        otherExtraHours: form.otherExtraHours,
        chiefType: form.chiefType,
        employmentType: form.employmentType,
      },
    },
  }
}

function startPreview(): void {
  if (!props.teacher || !isValid.value) return
  void change.preview(buildRequest(props.teacher))
}

function onImpactCancel(): void {
  // Etki penceresi kapatıldı; kullanıcı forma dönüp tekrar dener.
}

function close(): void {
  change.reset()
  emit('update:visible', false)
}

function onVisibleChange(next: boolean): void {
  if (!next) close()
}

// Pencere açılırken ya da öğretmen değişirken form, öğretmenin bugünkü yük
// değerleriyle doldurulur; gerekçe ve tarih her açılışta sıfırlanır.
watch(
  () => [props.visible, props.teacher] as const,
  ([isVisible, teacher]) => {
    if (!isVisible) return
    Object.assign(form, teacher ? loadFormFromTeacher(teacher) : emptyForm())
    effectiveDate.value = null
    reason.value = ''
    change.reset()
  },
  { immediate: true },
)

// Değişiklik kaydedilince pencere kapanır ve liste yenilensin diye üst bileşene bildirilir.
watch(
  () => change.status.value,
  (status) => {
    if (status !== 'committed') return
    emit('saved')
    close()
  },
)
</script>

<style scoped>
.form-grid { display: flex; flex-direction: column; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; flex: 1; }
.field-row { display: flex; gap: 1rem; }
label { font-size: 0.875rem; font-weight: 500; }
.teacher-name { font-weight: 600; margin: 0; }
.field-error { color: var(--p-red-500); font-size: 0.75rem; }
</style>
