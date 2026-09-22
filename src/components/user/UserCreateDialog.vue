<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :header="labels.settings.users.add"
    :style="{ width: '26rem' }"
  >
    <div class="form-grid">
      <div class="field">
        <label for="user-create-name">{{ labels.auth.name }}</label>
        <InputText id="user-create-name" v-model="form.name" autofocus fluid :aria-label="labels.auth.name" />
      </div>
      <div class="field">
        <label for="user-create-pin">{{ labels.auth.pin }}</label>
        <Password
          input-id="user-create-pin"
          v-model="pinProxy"
          :feedback="false"
          fluid
          :aria-label="labels.auth.pin"
          :input-props="pinInputProps"
        />
      </div>
      <div class="field">
        <label for="user-create-pin-confirm">{{ labels.auth.pinConfirm }}</label>
        <Password
          input-id="user-create-pin-confirm"
          v-model="pinConfirmProxy"
          :feedback="false"
          fluid
          :aria-label="labels.auth.pinConfirm"
          :input-props="pinInputProps"
        />
      </div>
    </div>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        data-testid="user-create-cancel-button"
        @click="close"
      />
      <Button
        :label="labels.common.save"
        :disabled="!isValid"
        data-testid="user-create-save-button"
        @click="save"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { labels } from '../../i18n/labels'
import { sanitizePinInput } from '../../utils/pin'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [name: string, pin: string]
}>()

const toast = useToast()

/** Sayısal tuş takımı ve tarayıcı otomatik doldurmasını kapatmak için; doğrulama içermez. */
const pinInputProps = { inputmode: 'numeric' as const, autocomplete: 'off', maxlength: 6 }

function emptyForm(): { name: string; pin: string; pinConfirm: string } {
  return { name: '', pin: '', pinConfirm: '' }
}

const form = reactive(emptyForm())

const pinProxy = computed({
  get: (): string => form.pin,
  set: (value: string | null | undefined): void => {
    form.pin = sanitizePinInput(value)
  },
})

const pinConfirmProxy = computed({
  get: (): string => form.pinConfirm,
  set: (value: string | null | undefined): void => {
    form.pinConfirm = sanitizePinInput(value)
  },
})

const isValid = computed(() => form.name.trim().length > 0 && form.pin.length > 0)

watch(
  () => props.visible,
  (visible) => {
    if (visible) Object.assign(form, emptyForm())
  },
)

function close(): void {
  emit('update:visible', false)
}

/** PIN tekrarının eşleşmediği tek kural arayüze aittir; geri kalanı arka uç doğrular. */
function save(): void {
  if (form.pin !== form.pinConfirm) {
    toast.add({ severity: 'warn', summary: labels.common.error, detail: labels.auth.pinMismatch, life: 5000 })
    return
  }
  emit('save', form.name.trim(), form.pin)
  close()
}
</script>

<style scoped>
.form-grid { display: flex; flex-direction: column; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; }
label { font-size: 0.875rem; font-weight: 500; }
</style>
