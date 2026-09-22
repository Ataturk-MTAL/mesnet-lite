<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :header="labels.settings.users.changePin"
    :style="{ width: '24rem' }"
  >
    <!-- Eski PIN sorulmaz: herkes tam yetkili, ekip üç kişi, veritabanı zaten
         korumasız — eski PIN sormak gerçek olmayan bir güvenlik hissi verir. -->
    <div class="form-grid">
      <div class="field">
        <label for="user-pin-new">{{ labels.settings.users.newPin }}</label>
        <Password
          input-id="user-pin-new"
          v-model="pinProxy"
          :feedback="false"
          fluid
          autofocus
          :aria-label="labels.settings.users.newPin"
          :input-props="pinInputProps"
        />
      </div>
      <div class="field">
        <label for="user-pin-new-confirm">{{ labels.settings.users.newPinConfirm }}</label>
        <Password
          input-id="user-pin-new-confirm"
          v-model="pinConfirmProxy"
          :feedback="false"
          fluid
          :aria-label="labels.settings.users.newPinConfirm"
          :input-props="pinInputProps"
        />
      </div>
    </div>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        data-testid="user-pin-cancel-button"
        @click="close"
      />
      <Button
        :label="labels.common.save"
        :disabled="pin.length === 0"
        data-testid="user-pin-save-button"
        @click="save"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { labels } from '../../i18n/labels'
import { sanitizePinInput } from '../../utils/pin'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [pin: string]
}>()

const toast = useToast()

/** Sayısal tuş takımı ve tarayıcı otomatik doldurmasını kapatmak için; doğrulama içermez. */
const pinInputProps = { inputmode: 'numeric' as const, autocomplete: 'off', maxlength: 6 }

const pin = ref('')
const pinConfirm = ref('')

const pinProxy = computed({
  get: (): string => pin.value,
  set: (value: string | null | undefined): void => {
    pin.value = sanitizePinInput(value)
  },
})

const pinConfirmProxy = computed({
  get: (): string => pinConfirm.value,
  set: (value: string | null | undefined): void => {
    pinConfirm.value = sanitizePinInput(value)
  },
})

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      pin.value = ''
      pinConfirm.value = ''
    }
  },
)

function close(): void {
  emit('update:visible', false)
}

/** PIN tekrarının eşleşmediği tek kural arayüze aittir; geri kalanı arka uç doğrular. */
function save(): void {
  if (pin.value !== pinConfirm.value) {
    toast.add({ severity: 'warn', summary: labels.common.error, detail: labels.auth.pinMismatch, life: 5000 })
    return
  }
  emit('save', pin.value)
  close()
}
</script>

<style scoped>
.form-grid { display: flex; flex-direction: column; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; }
label { font-size: 0.875rem; font-weight: 500; }
</style>
