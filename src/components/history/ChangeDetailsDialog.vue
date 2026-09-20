<template>
  <Dialog
    :visible="visible"
    @update:visible="onVisibleChange"
    modal
    :header="title"
    :style="{ width: '30rem' }"
    data-testid="change-details-dialog"
  >
    <div class="change-details-body">
      <EffectiveDateField
        v-model="effectiveDate"
        :label="labels.effectiveDateField.generic"
        :term="term"
        :show-error="hasAttempted"
      />

      <div class="change-details-field">
        <label :for="reasonId">{{ labels.history.reason }}</label>
        <Textarea
          :id="reasonId"
          v-model="reason"
          autoResize
          rows="3"
          :placeholder="labels.history.reasonPlaceholder"
          :aria-label="labels.history.reason"
          data-testid="change-details-reason"
          @blur="isReasonTouched = true"
        />
        <small v-if="showReasonError" class="change-details-error" data-testid="change-details-reason-error">
          {{ labels.history.reasonRequired }}
        </small>
      </div>
    </div>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        data-testid="change-details-cancel-button"
        @click="emit('cancel')"
      />
      <Button
        :label="labels.common.save"
        data-testid="change-details-confirm-button"
        @click="onConfirm"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { labels } from '../../i18n/labels'
import EffectiveDateField from './EffectiveDateField.vue'
import type { TermWithDates } from '../../types/models'

const props = defineProps<{
  visible: boolean
  term: TermWithDates
  /** Pencere her açılışında bu değerlerle başlar; düzeltme modunda önceden dolu gelir. */
  effectiveDate: string | null
  reason: string
  title: string
}>()

const emit = defineEmits<{
  confirm: [details: { effectiveDate: string | null; reason: string }]
  cancel: []
}>()

const effectiveDate = ref<string | null>(props.effectiveDate)
const reason = ref(props.reason)
const reasonId = useId()

/** Kaydet'e basmayı denedikten sonra zorunlu alan uyarıları görünür. */
const hasAttempted = ref(false)
const isReasonTouched = ref(false)

// Planlamada tarih hiç sorulmaz (EffectiveDateField gizlenir), dolayısıyla zorunlu da değildir.
const isDateMissing = computed(() => !props.term.isPlanning && effectiveDate.value === null)
const isReasonMissing = computed(() => reason.value.trim().length === 0)
const showReasonError = computed(() => isReasonMissing.value && (hasAttempted.value || isReasonTouched.value))

// Her açılışta başlangıç değerleri yüklenir ve uyarılar sıfırlanır.
watch(
  () => props.visible,
  (isVisible) => {
    if (!isVisible) return
    effectiveDate.value = props.effectiveDate
    reason.value = props.reason
    hasAttempted.value = false
    isReasonTouched.value = false
  },
  { immediate: true },
)

function onVisibleChange(next: boolean): void {
  if (!next) emit('cancel')
}

function onConfirm(): void {
  if (isDateMissing.value || isReasonMissing.value) {
    hasAttempted.value = true
    return
  }
  emit('confirm', { effectiveDate: effectiveDate.value, reason: reason.value.trim() })
}
</script>

<style scoped>
.change-details-body { display: flex; flex-direction: column; gap: 1rem; }
.change-details-field { display: flex; flex-direction: column; gap: 0.375rem; }
.change-details-field label { font-size: 0.875rem; font-weight: 500; }
.change-details-error { color: var(--p-red-500); font-size: 0.75rem; }
</style>
