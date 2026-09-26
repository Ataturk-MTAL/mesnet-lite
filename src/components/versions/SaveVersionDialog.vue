<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :header="labels.versions.saveDialogTitle"
    :style="{ width: '24rem' }"
  >
    <div class="field">
      <label for="version-save-name">{{ labels.versions.nameLabel }}</label>
      <InputText
        id="version-save-name"
        v-model="name"
        autofocus
        fluid
        :placeholder="labels.versions.namePlaceholder"
        :disabled="saving"
        :aria-label="labels.versions.nameLabel"
        @keyup.enter="save"
      />
    </div>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        data-testid="version-save-cancel-button"
        @click="close"
      />
      <!-- `disabled` `saving`'i de kapsar: OpenVue Button, açık bir `disabled`
           değeri geldiğinde `loading`'in otomatik devre dışı bırakmasını
           geçersiz kılıyor. UserRenameDialog.vue'daki not ile aynı sebep. -->
      <Button
        :label="labels.versions.save"
        :disabled="name.trim().length === 0 || saving"
        :loading="saving"
        data-testid="version-save-save-button"
        @click="save"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { labels } from '../../i18n/labels'

const props = defineProps<{ visible: boolean; saving: boolean }>()
const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [name: string]
}>()

const name = ref('')

// Her açılışta boş bir ad ile başlar; önceki denemenin adı kalmaz.
watch(
  () => props.visible,
  (visible) => {
    if (visible) name.value = ''
  },
)

function close(): void {
  emit('update:visible', false)
}

/** Diyalog KENDİNİ KAPATMAZ — kapanışı çağıran görünüm üstlenir; arka uç
 * reddederse diyalog açık kalır ve hata Toast ile gösterilir. */
function save(): void {
  if (name.value.trim().length === 0 || props.saving) return
  emit('save', name.value.trim())
}
</script>

<style scoped>
.field { display: flex; flex-direction: column; gap: 0.375rem; }
label { font-size: 0.875rem; font-weight: 500; }
</style>
