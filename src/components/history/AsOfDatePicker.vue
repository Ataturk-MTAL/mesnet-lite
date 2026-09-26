<template>
  <div class="as-of-date-picker">
    <label :for="inputId">{{ labels.asOfDate.label }}</label>
    <div class="as-of-date-row">
      <DatePicker
        :inputId="inputId"
        v-model="dateValue"
        dateFormat="dd.mm.yy"
        showIcon
        iconDisplay="input"
        :minDate="minDateValue"
        :maxDate="maxDateValue"
        :aria-label="labels.asOfDate.label"
        data-testid="as-of-date-picker"
      />
      <Button
        v-if="isReadOnly"
        :label="labels.asOfDate.today"
        severity="secondary"
        outlined
        size="small"
        :aria-label="labels.asOfDate.today"
        data-testid="as-of-today-button"
        @click="asOfDateStore.goToToday"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, useId } from 'vue'
import { storeToRefs } from 'pinia'
import { labels } from '../../i18n/labels'
import { useAsOfDateStore } from '../../stores/asOfDate'
import { dateToIso, isoToDate } from '../../utils/isoDate'

const inputId = useId()
const asOfDateStore = useAsOfDateStore()
const { asOfDate, isReadOnly, termStartDate, termEndDate } = storeToRefs(asOfDateStore)

const dateValue = computed({
  get: () => isoToDate(asOfDate.value),
  set: (value) => {
    const iso = dateToIso(value)
    if (iso) asOfDateStore.setAsOfDate(iso)
  },
})

// Seçilebilir aralık dönem başı–sonu ile sınırlanır.
const minDateValue = computed(() => isoToDate(termStartDate.value) ?? undefined)
const maxDateValue = computed(() => isoToDate(termEndDate.value) ?? undefined)
</script>

<style scoped>
.as-of-date-picker { display: flex; flex-direction: column; gap: 0.375rem; }
.as-of-date-row { display: flex; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
</style>
