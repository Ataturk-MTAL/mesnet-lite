<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :header="labels.settings.users.rename"
    :style="{ width: '24rem' }"
  >
    <div class="field">
      <label for="user-rename-name">{{ labels.auth.name }}</label>
      <InputText
        id="user-rename-name"
        v-model="name"
        autofocus
        fluid
        :disabled="saving"
        :aria-label="labels.auth.name"
      />
    </div>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        data-testid="user-rename-cancel-button"
        @click="close"
      />
      <!-- `disabled` `saving`'i de kapsar: OpenVue Button, açık bir `disabled`
           değeri geldiğinde `loading`'in otomatik devre dışı bırakmasını
           geçersiz kılıyor. UserCreateDialog.vue'daki not ile aynı sebep. -->
      <Button
        :label="labels.common.save"
        :disabled="name.trim().length === 0 || saving"
        :loading="saving"
        data-testid="user-rename-save-button"
        @click="save"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { labels } from '../../i18n/labels'
import type { User } from '../../types/models'

const props = defineProps<{ visible: boolean; user: User | null; saving: boolean }>()
const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [name: string]
}>()

const name = ref('')

watch(
  () => [props.visible, props.user] as const,
  ([visible, user]) => {
    if (visible) name.value = user?.name ?? ''
  },
  { immediate: true },
)

function close(): void {
  emit('update:visible', false)
}

/** Diyalog KENDİNİ KAPATMAZ — kapanışı `SettingsView` üstlenir. */
function save(): void {
  if (name.value.trim().length === 0) return
  emit('save', name.value.trim())
}
</script>

<style scoped>
.field { display: flex; flex-direction: column; gap: 0.375rem; }
label { font-size: 0.875rem; font-weight: 500; }
</style>
