<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :header="isEdit ? labels.common.edit : labels.common.add"
    :style="{ width: '34rem' }"
  >
    <div class="form-grid">
      <div class="field">
        <label for="company-name">{{ labels.company.name }}</label>
        <InputText id="company-name" v-model="form.name" autofocus />
      </div>

      <div class="field">
        <label for="company-address">{{ labels.company.address }}</label>
        <Textarea id="company-address" v-model="form.addressText" rows="2" />
      </div>

      <div class="field-row">
        <div class="field">
          <label for="company-contact-first">{{ labels.company.contactFirstName }}</label>
          <InputText id="company-contact-first" v-model="form.contactFirstName" />
        </div>
        <div class="field">
          <label for="company-contact-last">{{ labels.company.contactLastName }}</label>
          <InputText id="company-contact-last" v-model="form.contactLastName" />
        </div>
      </div>

      <div class="field-row">
        <div class="field">
          <label for="company-phone">{{ labels.company.phone }}</label>
          <InputText id="company-phone" v-model="form.phone" />
        </div>
        <div class="field">
          <label for="company-email">{{ labels.company.email }}</label>
          <InputText id="company-email" v-model="form.email" />
        </div>
      </div>

      <div class="field">
        <label for="company-distance">{{ labels.company.oneWayDistance }}</label>
        <InputNumber
          id="company-distance"
          v-model="form.oneWayDistanceKm"
          :min-fraction-digits="1"
          :max-fraction-digits="1"
          :min="0"
        />
        <small class="hint">{{ labels.company.distanceHint }}</small>
      </div>

      <div class="field">
        <label for="company-notes">{{ labels.company.notes }}</label>
        <Textarea id="company-notes" v-model="form.notes" rows="2" />
      </div>
    </div>

    <template #footer>
      <Button :label="labels.common.cancel" severity="secondary" outlined @click="close" />
      <Button :label="labels.common.save" :disabled="!isValid" @click="save" />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { labels } from '../../i18n/labels'
import type { Company, NewCompany } from '../../types/models'

const props = defineProps<{ visible: boolean; company: Company | null }>()
const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [input: NewCompany]
}>()

function emptyForm(): NewCompany {
  return {
    name: '',
    contactFirstName: '',
    contactLastName: '',
    phone: '',
    email: '',
    addressText: '',
    latitude: null,
    longitude: null,
    oneWayDistanceKm: null,
    notes: '',
  }
}

function toForm(company: Company): NewCompany {
  return {
    name: company.name,
    contactFirstName: company.contactFirstName,
    contactLastName: company.contactLastName,
    phone: company.phone,
    email: company.email,
    addressText: company.addressText,
    latitude: company.latitude,
    longitude: company.longitude,
    oneWayDistanceKm: company.oneWayDistanceKm,
    notes: company.notes,
  }
}

const form = reactive<NewCompany>(emptyForm())
const isEdit = computed(() => props.company !== null)
const isValid = computed(
  () => form.name.trim().length > 0 && form.addressText.trim().length > 0,
)

// Diyalog her açılışta seçili kayıttan doldurulur.
watch(
  () => [props.visible, props.company] as const,
  ([isVisible, company]) => {
    if (!isVisible) return
    Object.assign(form, company ? toForm(company) : emptyForm())
  },
  { immediate: true },
)

function close(): void {
  emit('update:visible', false)
}

function save(): void {
  emit('save', { ...form })
  close()
}
</script>

<style scoped>
.form-grid { display: flex; flex-direction: column; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; flex: 1; }
.field-row { display: flex; gap: 1rem; }
label { font-size: 0.875rem; font-weight: 500; }
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
</style>
