<template>
  <div class="page">
    <h1 class="page-title">{{ labels.nav.settings }}</h1>

    <Card>
      <template #title>{{ labels.settings.schoolSection }}</template>
      <template #content>
        <div class="grid">
          <div class="field field--wide">
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
            <label for="max-daily-lessons">{{ labels.settings.maxDailyLessons }}</label>
            <InputNumber
              input-id="max-daily-lessons"
              v-model="form.maxDailyLessons"
              :min="MIN_DAILY_LESSONS"
              :max="MAX_DAILY_LESSONS"
              fluid
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
import { computed, onMounted, reactive, ref } from 'vue'
import { useToast } from 'openvue/usetoast'
import LocationPickerMap from '../components/map/LocationPickerMap.vue'
import { settingsApi } from '../api/settings'
import type { SettingsMap } from '../api/settings'
import { labels } from '../i18n/labels'
import type { LatLng } from '../types/models'

const toast = useToast()

/** Günlük ders saati sayısının alt sınırı. Izgara ders numarası artık her zaman 1'den başlar. */
const MIN_DAILY_LESSONS = 1
/** Ders numarası 1'den başladığı için günün 24 saatini aşmayacak azami sayı: 24 - 1 = 23. */
const HOURS_IN_DAY = 24
const GRID_START_LESSON = 1
const MAX_DAILY_LESSONS = HOURS_IN_DAY - GRID_START_LESSON
/** Ayar anahtarı bulunmayan kurulumlarda bugünkü varsayılan: 9 ders saati. */
const DEFAULT_DAILY_LESSONS = 9

const form = reactive({
  schoolName: '',
  principalName: '',
  fieldName: '',
  activeTerm: '',
  institutionType: 'other',
  isMetropolitanDistrict: true,
  maxDailyLessons: DEFAULT_DAILY_LESSONS as number | null,
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

function parsePositiveInt(value: string | undefined): number | null {
  const parsed = Number.parseInt(value ?? '', 10)
  return Number.isNaN(parsed) || parsed < MIN_DAILY_LESSONS ? null : parsed
}

/**
 * Günlük azami ders saati: önce `max_daily_lessons`; yoksa (eski kurulum, göç öncesi) eski
 * `day_end_hour - day_start_hour` çifti; ikisi de yoksa bugünkü varsayılan. Rust tarafı artık
 * mevcut veriyi göçle 1..N numaralandırmasına taşıyor; bu geri dönüş yalnız göç öncesi veya
 * eksik ayar durumunda devreye girer.
 */
function resolveMaxDailyLessons(settings: SettingsMap): number {
  const stored = parsePositiveInt(settings.max_daily_lessons)
  if (stored !== null) return stored
  const startHour = Number.parseInt(settings.day_start_hour ?? '', 10)
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
  form.maxDailyLessons = resolveMaxDailyLessons(settings)
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
  if (lessons === null || lessons < MIN_DAILY_LESSONS || lessons > MAX_DAILY_LESSONS) {
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
      // Gün başlangıç ve bitiş saatleri artık burada yazılmıyor; aralığı Rust tarafı
      // `max_daily_lessons`'tan türetiyor (ders numarası her zaman 1'den başlar).
      max_daily_lessons: String(lessons),
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

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.grid { display: flex; flex-wrap: wrap; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; flex: 1 1 16rem; max-width: 26rem; min-width: 0; }
/* Okul adı uzun olabilir; diğer alanlardan geniş tutulur ki kırpılmasın. */
.field--wide { flex-basis: 24rem; max-width: 40rem; }
.field--inline { flex: 0 0 auto; flex-direction: row; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
.cap-note { margin-top: 1rem; }
.actions { display: flex; justify-content: flex-end; }
</style>
