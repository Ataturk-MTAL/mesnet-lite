<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :closable="change.status.value === 'idle'"
    :close-on-escape="change.status.value === 'idle'"
    :header="labels.studentChange.title"
    :style="{ width: '32rem' }"
  >
    <div class="form-grid">
      <div class="field">
        <label>{{ labels.studentChange.fromCompany }}</label>
        <p class="readonly-value">{{ student.companyName ?? labels.student.noCompany }}</p>
      </div>

      <div class="field">
        <SelectButton
          ref="modeSelect"
          :model-value="mode"
          :options="modeOptions"
          optionLabel="label"
          optionValue="value"
          :allowEmpty="false"
          :aria-label="labels.studentChange.title"
          @update:model-value="(value: ChangeMode) => (mode = value)"
        />
      </div>

      <template v-if="mode === 'transfer'">
        <div class="field">
          <label>{{ labels.studentChange.targetCompany }}</label>
          <SelectButton
            ref="targetModeSelect"
            :model-value="targetMode"
            :options="targetModeOptions"
            optionLabel="label"
            optionValue="value"
            :allowEmpty="false"
            :aria-label="labels.studentChange.targetCompany"
            @update:model-value="(value: TargetMode) => (targetMode = value)"
          />
        </div>

        <div v-if="targetMode === 'existing'" class="field">
          <Select
            ref="companySelect"
            :model-value="targetCompanyId"
            :options="companyOptions"
            optionLabel="label"
            optionValue="value"
            :placeholder="labels.studentChange.targetCompany"
            :aria-label="labels.studentChange.targetCompany"
            filter
            @update:model-value="(value: number | null) => (targetCompanyId = value)"
          />
        </div>

        <div v-else class="field">
          <p v-if="newCompany" class="readonly-value">{{ newCompany.name }}</p>
          <Button
            :label="labels.studentChange.targetCompanyNew"
            severity="secondary"
            outlined
            size="small"
            @click="isCompanyDialogOpen = true"
          />
        </div>
      </template>

      <EffectiveDateField ref="dateField" v-model="effectiveDate" :label="dateLabel" :term="term" />

      <div class="field">
        <label :for="reasonId">{{ labels.history.reason }}</label>
        <Textarea
          :id="reasonId"
          ref="reasonField"
          v-model="reason"
          rows="2"
          :placeholder="labels.studentChange.reasonPlaceholder"
          :aria-label="labels.history.reason"
        />
        <small v-if="isReasonEmpty" class="field-error">{{ labels.history.reasonRequired }}</small>
      </div>
    </div>

    <template #footer>
      <Button :label="labels.common.cancel" severity="secondary" outlined @click="close" />
      <Button
        :label="labels.common.save"
        :disabled="isSubmitDisabled"
        data-testid="student-change-submit-button"
        @click="submit"
      />
    </template>
  </Dialog>

  <CompanyFormDialog v-model:visible="isCompanyDialogOpen" :company="null" @save="handleNewCompanySave" />

  <ImpactDialog :change="change" />
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { labels } from '../../i18n/labels'
import { useChange } from '../../composables/useChange'
import { companiesApi } from '../../api/companies'
import type { ChangeCommand, ChangeRequest, Company, NewCompany, TermWithDates, TransferTarget } from '../../types/models'

/**
 * Öğrenci nakli/ayrılışı için dinamik özne. `Student` tipinden farklı olarak
 * mevcut işletme adını da taşır; çağıran ekran bunu kendi işletme listesinden
 * çözer (spec §7: "mevcut işletme adı").
 */
export interface StudentChangeSubject {
  id: number
  fullName: string
  companyId: number | null
  companyName: string | null
}

type ChangeMode = 'transfer' | 'leave'
type TargetMode = 'existing' | 'new'

const props = defineProps<{
  visible: boolean
  student: StudentChangeSubject
  term: TermWithDates
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  /** İstek `committed` sonucuna ulaştığında; üst ekran listeyi yeniden yükler. */
  saved: []
}>()

const toast = useToast()
const change = useChange()

const mode = ref<ChangeMode>('transfer')
const targetMode = ref<TargetMode>('existing')
const targetCompanyId = ref<number | null>(null)
const newCompany = ref<NewCompany | null>(null)
const isCompanyDialogOpen = ref(false)
const effectiveDate = ref<string | null>(null)
const reason = ref('')
const companies = ref<Company[]>([])

const reasonId = useId()

const modeOptions = [
  { value: 'transfer' as const, label: labels.studentChange.transferOption },
  { value: 'leave' as const, label: labels.studentChange.leaveOption },
]
const targetModeOptions = [
  { value: 'existing' as const, label: labels.studentChange.targetCompany },
  { value: 'new' as const, label: labels.studentChange.targetCompanyNew },
]

// Öğrenci hâlihazırda bu işletmede olduğu için hedef listesinden çıkarılır.
const companyOptions = computed(() =>
  companies.value
    .filter((company) => company.id !== props.student.companyId)
    .map((company) => ({ value: company.id, label: company.name })),
)

const dateLabel = computed(() =>
  mode.value === 'leave' ? labels.effectiveDateField.contractEnd : labels.effectiveDateField.contractStart,
)

const isReasonEmpty = computed(() => reason.value.trim().length === 0)
const isDateMissing = computed(() => !props.term.isPlanning && effectiveDate.value === null)
const isTargetMissing = computed(() => {
  if (mode.value !== 'transfer') return false
  return targetMode.value === 'existing' ? targetCompanyId.value === null : newCompany.value === null
})

const isFormValid = computed(
  () => props.student.companyId !== null && !isReasonEmpty.value && !isDateMissing.value && !isTargetMissing.value,
)

const isSubmitDisabled = computed(() => !isFormValid.value || change.status.value !== 'idle')

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

async function loadCompanies(): Promise<void> {
  try {
    companies.value = await companiesApi.list()
  } catch (error: unknown) {
    showError(error)
  }
}

function resetForm(): void {
  mode.value = 'transfer'
  targetMode.value = 'existing'
  targetCompanyId.value = null
  newCompany.value = null
  effectiveDate.value = null
  reason.value = ''
  change.reset()
}

// Diyalog her açılışta temiz bir formla başlar; işletme listesi tazelenir.
watch(
  () => props.visible,
  (isVisible) => {
    if (!isVisible) return
    resetForm()
    void loadCompanies()
  },
  { immediate: true },
)

// Kayıt tamamlanınca üst ekrana haber verilir ve pencere kapanır; etki
// penceresi (`ImpactDialog`) kendi durumunu koruduğu için ayrıca kapatılmaz.
watch(
  () => change.status.value,
  (status) => {
    if (status !== 'committed') return
    emit('saved')
    emit('update:visible', false)
  },
)

function handleNewCompanySave(input: NewCompany): void {
  newCompany.value = input
}

function buildTarget(): TransferTarget | null {
  if (targetMode.value === 'existing') {
    return targetCompanyId.value === null ? null : { type: 'existing', companyId: targetCompanyId.value }
  }
  return newCompany.value === null ? null : { type: 'new', company: newCompany.value }
}

function buildCommand(): ChangeCommand | null {
  const fromCompanyId = props.student.companyId
  if (fromCompanyId === null) return null
  if (mode.value === 'leave') {
    return { type: 'studentLeaves', studentId: props.student.id, fromCompanyId }
  }
  const to = buildTarget()
  return to === null ? null : { type: 'transferStudent', studentId: props.student.id, fromCompanyId, to }
}

async function submit(): Promise<void> {
  const command = buildCommand()
  if (command === null) return
  const request: ChangeRequest = {
    term: props.term.term,
    effectiveDate: effectiveDate.value,
    documentDate: null,
    reason: reason.value.trim(),
    command,
  }
  await change.preview(request)
}

function close(): void {
  emit('update:visible', false)
}
</script>

<style scoped>
.form-grid { display: flex; flex-direction: column; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; }
label { font-size: 0.875rem; font-weight: 500; }
.readonly-value { margin: 0; font-size: 0.9375rem; }
.field-error { color: var(--p-red-500); font-size: 0.75rem; }
</style>
