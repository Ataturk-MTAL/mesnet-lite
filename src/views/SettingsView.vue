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
            <label for="principal-name">{{ labels.settings.principalName }}</label>
            <InputText
              id="principal-name"
              v-model="form.principalName"
              aria-describedby="principal-name-help"
            />
            <small id="principal-name-help" class="hint">{{ labels.settings.blankPrintsDotted }}</small>
          </div>

          <div class="field">
            <label for="field-name">{{ labels.settings.fieldName }}</label>
            <InputText id="field-name" v-model="form.fieldName" aria-describedby="field-name-help" />
            <small id="field-name-help" class="hint">{{ labels.settings.blankPrintsDotted }}</small>
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
            <label for="max-daily-lessons">{{ labels.settings.maxDailyLessons }}</label>
            <InputNumber
              input-id="max-daily-lessons"
              v-model="form.maxDailyLessons"
              :min="MIN_DAILY_LESSONS"
              :max="maxLessonsUpperBound"
              :aria-label="labels.settings.maxDailyLessons"
            />
            <small class="hint">{{ labels.settings.maxDailyLessonsHint }}</small>
          </div>
        </div>
      </template>
    </Card>

    <div class="actions">
      <Button :label="labels.common.save" icon="pi pi-check" :loading="isSaving" @click="save" />
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

/** Günlük ders saati sayısının alt sınırı ve günün son saati (başlangıç + sayı bunu aşamaz). */
const MIN_DAILY_LESSONS = 1
const HOURS_IN_DAY = 24
/** Ayar anahtarı bulunmayan kurulumlarda bugünkü varsayılan: 8..16 = 9 saat. */
const DEFAULT_DAILY_LESSONS = 9

const form = reactive({
  schoolName: '',
  principalName: '',
  fieldName: '',
  activeTerm: '',
  institutionType: 'other',
  isMetropolitanDistrict: true,
  dayStartHour: 8,
  maxDailyLessons: DEFAULT_DAILY_LESSONS as number | null,
})

/** InputNumber'ın kabul ettiği azami sayı; başlangıç saatiyle birlikte 24'ü aşamaz. */
const maxLessonsUpperBound = computed<number>(() => HOURS_IN_DAY - (form.dayStartHour ?? 0))

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

function parsePositiveInt(value: string | undefined): number | null {
  const parsed = Number.parseInt(value ?? '', 10)
  return Number.isNaN(parsed) || parsed < MIN_DAILY_LESSONS ? null : parsed
}

/**
 * Günlük azami ders saati: önce `max_daily_lessons`; yoksa (eski kurulum) `day_end_hour - day_start_hour`;
 * ikisi de yoksa bugünkü varsayılan. Böylece mevcut kullanıcı verisinin davranışı değişmez.
 */
function resolveMaxDailyLessons(settings: SettingsMap, startHour: number): number {
  const stored = parsePositiveInt(settings.max_daily_lessons)
  if (stored !== null) return stored
  const endHour = Number.parseInt(settings.day_end_hour ?? '', 10)
  const derived = endHour - startHour
  return Number.isNaN(derived) || derived < MIN_DAILY_LESSONS ? DEFAULT_DAILY_LESSONS : derived
}

function applySettings(settings: SettingsMap): void {
  form.schoolName = settings.school_name ?? ''
  form.principalName = settings.principal_name ?? ''
  form.fieldName = settings.field_name ?? ''
  form.activeTerm = settings.active_term ?? ''
  form.institutionType = settings.institution_type ?? 'other'
  form.isMetropolitanDistrict = settings.is_metropolitan_district === 'true'
  form.dayStartHour = Number.parseInt(settings.day_start_hour ?? '8', 10)
  form.maxDailyLessons = resolveMaxDailyLessons(settings, form.dayStartHour)
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
  const lessons = form.maxDailyLessons
  if (lessons === null || lessons < MIN_DAILY_LESSONS || form.dayStartHour + lessons > HOURS_IN_DAY) {
    toast.add({
      severity: 'warn',
      summary: labels.common.error,
      detail: labels.settings.maxDailyLessonsInvalid,
      life: 5000,
    })
    return
  }

  isSaving.value = true
  try {
    const entries: SettingsMap = {
      school_name: form.schoolName,
      principal_name: form.principalName,
      field_name: form.fieldName,
      active_term: form.activeTerm,
      institution_type: form.institutionType,
      is_metropolitan_district: String(form.isMetropolitanDistrict),
      day_start_hour: String(form.dayStartHour),
      max_daily_lessons: String(lessons),
      // Rust okuyucuları ve ızgara `day_end_hour`'ı okur; satır sayısı = ders saati sayısı olsun diye türetilir.
      day_end_hour: String(form.dayStartHour + lessons),
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

// Başlangıç saati ilerleyince ders saati sayısı 24'ü aşacaksa sayı sınıra çekilir.
watch(
  () => form.dayStartHour,
  () => {
    const upper = maxLessonsUpperBound.value
    if (form.maxDailyLessons !== null && form.maxDailyLessons > upper) {
      form.maxDailyLessons = Math.max(MIN_DAILY_LESSONS, upper)
    }
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
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
.cap-note { margin-top: 1rem; }
.actions { display: flex; justify-content: flex-end; }
</style>
