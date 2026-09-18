<template>
  <div class="page">
    <h1 class="page-title">{{ labels.nav.settings }}</h1>

    <Card>
      <template #title>{{ labels.settings.schoolSection }}</template>
      <template #content>
        <div class="grid">
          <div class="field">
            <label for="school-name">{{ labels.settings.schoolName }}</label>
            <InputText id="school-name" v-model="form.schoolName" />
          </div>

          <div class="field">
            <label for="active-term">{{ labels.settings.activeTerm }}</label>
            <InputText id="active-term" v-model="form.activeTerm" />
          </div>

          <div class="field">
            <label for="institution-type">{{ labels.settings.institutionType }}</label>
            <Select
              id="institution-type"
              v-model="form.institutionType"
              :options="institutionTypeOptions"
              optionLabel="label"
              optionValue="value"
            />
          </div>

          <div class="field field--inline">
            <Checkbox
              inputId="metropolitan"
              v-model="form.isMetropolitanDistrict"
              :binary="true"
            />
            <label for="metropolitan">{{ labels.settings.isMetropolitanDistrict }}</label>
          </div>
        </div>

        <Message severity="info" :closable="false" class="cap-note">
          {{ labels.settings.statutoryCap }}: <strong>{{ statutoryCap }}</strong>
          {{ labels.settings.hoursPerWeek }} — {{ statutoryCapSource }}
        </Message>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.map.schoolLocation }}</template>
      <template #content>
        <LocationPickerMap v-model="schoolLocation" />
        <Message severity="secondary" :closable="false" class="cap-note">
          {{ labels.settings.schoolLocationNote }}
        </Message>
      </template>
    </Card>

    <Card>
      <template #title>{{ labels.settings.dayRangeSection }}</template>
      <template #content>
        <div class="grid">
          <div class="field">
            <label for="day-start">{{ labels.settings.dayStartHour }}</label>
            <InputNumber id="day-start" v-model="form.dayStartHour" :min="0" :max="23" />
          </div>
          <div class="field">
            <label for="day-end">{{ labels.settings.dayEndHour }}</label>
            <InputNumber id="day-end" v-model="form.dayEndHour" :min="1" :max="24" />
          </div>
        </div>
      </template>
    </Card>

    <div class="actions">
      <Button :label="labels.common.save" :loading="isSaving" @click="save" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import LocationPickerMap from '../components/map/LocationPickerMap.vue'
import { settingsApi } from '../api/settings'
import type { SettingsMap } from '../api/settings'
import { labels } from '../i18n/labels'
import type { LatLng } from '../types/models'

const toast = useToast()

const form = reactive({
  schoolName: '',
  activeTerm: '',
  institutionType: 'other',
  isMetropolitanDistrict: true,
  dayStartHour: 8,
  dayEndHour: 17,
})

const schoolLocation = ref<LatLng | null>(null)
const isSaving = ref(false)

const institutionTypeOptions = [
  { value: 'other', label: labels.institutionType.other },
  { value: 'vocational_center', label: labels.institutionType.vocational_center },
]

/**
 * MADDE 15/2 haftalık koordinatörlük tavanı.
 * a) Meslekî eğitim merkezlerinde: büyükşehir ilçesi 24, diğer 18
 * b) Diğer okul ve kurumlarda: büyükşehir ilçesi 20, diğer 16
 */
const statutoryCap = computed<number>(() => {
  const isCenter = form.institutionType === 'vocational_center'
  if (isCenter) return form.isMetropolitanDistrict ? 24 : 18
  return form.isMetropolitanDistrict ? 20 : 16
})

const statutoryCapSource = computed<string>(() => {
  const isCenter = form.institutionType === 'vocational_center'
  if (isCenter) return form.isMetropolitanDistrict ? 'MADDE 15/2-a-1' : 'MADDE 15/2-a-2'
  return form.isMetropolitanDistrict ? 'MADDE 15/2-b-1' : 'MADDE 15/2-b-2'
})

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 6000 })
}

function parseLocation(settings: SettingsMap): LatLng | null {
  const latitude = Number.parseFloat(settings.school_latitude ?? '')
  const longitude = Number.parseFloat(settings.school_longitude ?? '')
  if (Number.isNaN(latitude) || Number.isNaN(longitude)) return null
  return { latitude, longitude }
}

function applySettings(settings: SettingsMap): void {
  form.schoolName = settings.school_name ?? ''
  form.activeTerm = settings.active_term ?? ''
  form.institutionType = settings.institution_type ?? 'other'
  form.isMetropolitanDistrict = settings.is_metropolitan_district === 'true'
  form.dayStartHour = Number.parseInt(settings.day_start_hour ?? '8', 10)
  form.dayEndHour = Number.parseInt(settings.day_end_hour ?? '17', 10)
  schoolLocation.value = parseLocation(settings)
}

async function load(): Promise<void> {
  try {
    applySettings(await settingsApi.get())
  } catch (error: unknown) {
    showError(error)
  }
}

async function save(): Promise<void> {
  if (form.dayEndHour <= form.dayStartHour) {
    toast.add({
      severity: 'warn',
      summary: labels.common.error,
      detail: labels.settings.dayRangeInvalid,
      life: 5000,
    })
    return
  }

  isSaving.value = true
  try {
    const entries: SettingsMap = {
      school_name: form.schoolName,
      active_term: form.activeTerm,
      institution_type: form.institutionType,
      is_metropolitan_district: String(form.isMetropolitanDistrict),
      day_start_hour: String(form.dayStartHour),
      day_end_hour: String(form.dayEndHour),
      school_latitude: schoolLocation.value ? String(schoolLocation.value.latitude) : '',
      school_longitude: schoolLocation.value ? String(schoolLocation.value.longitude) : '',
    }
    applySettings(await settingsApi.save(entries))
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSaving.value = false
  }
}

// Gün aralığı tutarsızsa kullanıcı kaydetmeden önce uyarılır (save içinde),
// burada yalnızca bitişin başlangıcın altına düşmesi engellenir.
watch(
  () => form.dayStartHour,
  (start) => {
    if (form.dayEndHour <= start) form.dayEndHour = start + 1
  },
)

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.grid { display: flex; flex-wrap: wrap; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; min-width: 16rem; }
.field--inline { flex-direction: row; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
.cap-note { margin-top: 1rem; }
.actions { display: flex; justify-content: flex-end; }
</style>
