<template>
  <Dialog
    :visible="visible"
    @update:visible="close"
    modal
    :header="labels.termManagement.editDates"
    :style="{ width: '28rem' }"
    data-testid="term-dates-dialog"
  >
    <div class="form-grid" v-if="term">
      <p class="term-name">{{ term.term }}</p>

      <div class="field">
        <label :for="startId">{{ labels.termManagement.startDate }}</label>
        <DatePicker
          :inputId="startId"
          v-model="startDateValue"
          showIcon
          iconDisplay="input"
          dateFormat="dd.mm.yy"
          :disabled="saving"
          :aria-label="labels.termManagement.startDate"
          data-testid="term-dates-start-picker"
        />
      </div>

      <div class="field">
        <label :for="endId">{{ labels.termManagement.endDate }}</label>
        <DatePicker
          :inputId="endId"
          v-model="endDateValue"
          showIcon
          iconDisplay="input"
          dateFormat="dd.mm.yy"
          :disabled="saving"
          :aria-label="labels.termManagement.endDate"
          data-testid="term-dates-end-picker"
        />
      </div>

      <div class="field field--inline">
        <Checkbox
          :inputId="confirmedId"
          v-model="confirmed"
          :binary="true"
          :disabled="saving"
        />
        <label :for="confirmedId">{{ labels.termManagement.confirmCheckbox }}</label>
      </div>
    </div>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        :disabled="saving"
        data-testid="term-dates-cancel-button"
        @click="close"
      />
      <Button
        :label="labels.termManagement.saveDates"
        :loading="saving"
        :disabled="saving || isInvalid"
        data-testid="term-dates-save-button"
        @click="save"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { labels } from '../../i18n/labels'
import type { TermWithDates } from '../../types/models'
import { dateToIso, isoToDate, type DatePickerModelValue } from '../../utils/isoDate'

const props = defineProps<{
  visible: boolean
  /** Düzenlenen dönem; pencere kapalıyken ya da satır seçilmeden önce `null`. */
  term: TermWithDates | null
  /** Kayıt sürüyorken alanlar ve düğmeler devre dışı kalır, pencere açık kalır. */
  saving: boolean
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [payload: { startDate: string; endDate: string; confirmed: boolean }]
}>()

const startId = useId()
const endId = useId()
const confirmedId = useId()

const startDateIso = ref<string | null>(null)
const endDateIso = ref<string | null>(null)
const confirmed = ref(false)

const startDateValue = computed<DatePickerModelValue>({
  get: () => isoToDate(startDateIso.value),
  set: (value) => {
    startDateIso.value = dateToIso(value)
  },
})

const endDateValue = computed<DatePickerModelValue>({
  get: () => isoToDate(endDateIso.value),
  set: (value) => {
    endDateIso.value = dateToIso(value)
  },
})

// Sıra/geçmiş ay kurallarını burada YENİDEN yazmıyoruz — arka uç reddederse
// mesajı olduğu gibi gösteririz. Yalnızca iki alanın da dolu olması istenir.
const isInvalid = computed(() => startDateIso.value === null || endDateIso.value === null)

// Pencere her açılışta seçilen satırın güncel değerleriyle başlar.
watch(
  () => [props.visible, props.term] as const,
  ([visible, term]) => {
    if (!visible) return
    startDateIso.value = term?.startDate ?? null
    endDateIso.value = term?.endDate ?? null
    confirmed.value = term?.datesConfirmed ?? false
  },
  { immediate: true },
)

function close(): void {
  emit('update:visible', false)
}

/** Diyalog KENDİNİ KAPATMAZ — arka uç reddederse üst görünüm pencereyi açık bırakır. */
function save(): void {
  if (isInvalid.value || startDateIso.value === null || endDateIso.value === null) return
  emit('save', { startDate: startDateIso.value, endDate: endDateIso.value, confirmed: confirmed.value })
}
</script>

<style scoped>
.form-grid { display: flex; flex-direction: column; gap: 1rem; }
.term-name { margin: 0; font-weight: 600; }
.field { display: flex; flex-direction: column; gap: 0.375rem; }
.field--inline { flex-direction: row; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
</style>
