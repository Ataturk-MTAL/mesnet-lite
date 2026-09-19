<template>
  <div v-if="!term.isPlanning" class="effective-date-field">
    <label :for="inputId">{{ label }}</label>
    <DatePicker
      :inputId="inputId"
      v-model="dateValue"
      :minDate="minDate"
      :maxDate="maxDate"
      showIcon
      iconDisplay="input"
      dateFormat="dd.mm.yy"
      :aria-label="label"
      data-testid="effective-date-picker"
    />
    <small v-if="isEmpty" class="field-error">{{ labels.effectiveDateField.required }}</small>
    <small v-else class="hint">{{ labels.effectiveDateField.hint(term.earliestAllowedDate) }}</small>
  </div>
</template>

<script setup lang="ts">
import { computed, useId } from 'vue'
import { labels } from '../../i18n/labels'
import type { TermWithDates } from '../../types/models'
import { dateToIso, isoToDate, type DatePickerModelValue } from '../../utils/isoDate'

const props = defineProps<{
  modelValue: string | null
  /** Çözülmüş etiket metni; anahtar değil. Çağıran `labels.effectiveDateField.*`'ten seçip geçirir. */
  label: string
  term: TermWithDates
}>()

const emit = defineEmits<{ 'update:modelValue': [value: string | null] }>()

const inputId = useId()

const dateValue = computed<DatePickerModelValue>({
  get: () => isoToDate(props.modelValue),
  set: (value) => emit('update:modelValue', dateToIso(value)),
})

const minDate = computed(() => isoToDate(props.term.earliestAllowedDate) ?? undefined)
const maxDate = computed(() => isoToDate(props.term.endDate) ?? undefined)
const isEmpty = computed(() => props.modelValue === null || props.modelValue.trim().length === 0)
</script>

<style scoped>
.effective-date-field { display: flex; flex-direction: column; gap: 0.375rem; }
label { font-size: 0.875rem; font-weight: 500; }
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
.field-error { color: var(--p-red-500); font-size: 0.75rem; }
</style>
