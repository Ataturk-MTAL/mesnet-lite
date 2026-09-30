<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :header="isEdit ? labels.common.edit : labels.common.add"
    :style="{ width: '36rem' }"
  >
    <div class="form-grid">
      <div class="field-row">
        <div class="field">
          <label for="teacher-first">{{ labels.teacher.firstName }}</label>
          <InputText id="teacher-first" v-model="form.firstName" autofocus />
        </div>
        <div class="field">
          <label for="teacher-last">{{ labels.teacher.lastName }}</label>
          <InputText id="teacher-last" v-model="form.lastName" />
        </div>
      </div>

      <div class="field-row">
        <div class="field">
          <label for="teacher-registry">{{ labels.teacher.registryNo }}</label>
          <InputText id="teacher-registry" v-model="form.registryNo" />
        </div>
        <!-- İstihdam türü bir yük alanıdır; yalnız yeni kayıtta girilir. -->
        <div v-if="!isEdit" class="field">
          <label for="teacher-employment">{{ labels.teacher.employmentType }}</label>
          <Select
            id="teacher-employment"
            v-model="form.employmentType"
            :options="employmentOptions"
            optionLabel="label"
            optionValue="value"
          />
        </div>
      </div>

      <div class="field">
        <label for="teacher-field">{{ labels.teacher.field }}</label>
        <InputText id="teacher-field" v-model="form.field" />
      </div>

      <div class="field">
        <label for="teacher-branches">{{ labels.teacher.branches }}</label>
        <!-- Dallar serbest metindir: okulun dal listesi zamanla değişir ve
             CSV'den gelen dal adları birebir eşleşmelidir. -->
        <AutoComplete
          id="teacher-branches"
          v-model="form.branches"
          multiple
          :suggestions="branchSuggestions"
          :typeahead="false"
          @complete="onBranchComplete"
        />
        <small class="hint">{{ labels.teacher.branchesHint }}</small>
      </div>

      <!-- Yük alanları yalnız YENİ öğretmen oluşturulurken burada girilir; mevcut
           bir öğretmenin yükü artık tarihçeye yazan TeacherLoadDialog'dan değişir. -->
      <template v-if="!isEdit">
        <div class="field-row">
          <div class="field">
            <label for="teacher-chief">{{ labels.teacher.chiefType }}</label>
            <Select
              id="teacher-chief"
              :modelValue="form.chiefType"
              @update:modelValue="onChiefTypeChange"
              :options="chiefOptions"
              optionLabel="label"
              optionValue="value"
            />
            <small class="hint">
              {{ labels.teacher.chiefHours }}: {{ chiefHours }} — MADDE 6/4
            </small>
          </div>
          <div class="field">
            <label for="teacher-max-extra">{{ labels.teacher.maxExtraHours }}</label>
            <InputNumber id="teacher-max-extra" v-model="form.maxExtraHours" :min="0" :max="60" />
            <small class="hint">MADDE 6/1-c</small>
          </div>
        </div>

        <div class="field-row">
          <div class="field">
            <label for="teacher-base">{{ labels.teacher.baseHours }}</label>
            <InputNumber id="teacher-base" v-model="form.baseHours" :min="0" :max="40" />
            <small class="hint">MADDE 5/1-ç</small>
          </div>
          <div class="field">
            <label for="teacher-other">{{ labels.teacher.otherExtraHours }}</label>
            <InputNumber id="teacher-other" v-model="form.otherExtraHours" :min="0" :max="40" />
          </div>
        </div>

        <Message v-if="capacityWarning" severity="warn" :closable="false">
          {{ capacityWarning }}
        </Message>
      </template>

      <div class="field field--inline">
        <Checkbox inputId="teacher-active" v-model="form.isActive" :binary="true" />
        <label for="teacher-active">{{ labels.teacher.isActive }}</label>
      </div>
    </div>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        data-testid="teacher-form-cancel-button"
        @click="close"
      />
      <Button
        :label="labels.common.save"
        :disabled="!isValid"
        data-testid="teacher-form-save-button"
        @click="save"
      />
    </template>
  </Dialog>

  <!-- Dönem başladıktan sonra oluşturulan YENİ kayıt, yürürlük tarihi ve
       gerekçe girilmeden Rust tarafından reddedilir; bu pencere onları burada
       sorar. Düzenlemede hiç açılmaz (yük tarihçeden değişir). -->
  <ChangeDetailsDialog
    :visible="isChangeDetailsOpen"
    :term="changeDetailsTerm"
    :title="labels.history.changeDetailsTitle"
    :effective-date="null"
    reason=""
    @confirm="onChangeDetailsConfirm"
    @cancel="isChangeDetailsOpen = false"
  />
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { labels } from '../../i18n/labels'
import { parseBranches } from '../../types/models'
import {
  CHIEF_HOURS_BY_TYPE,
  CHIEF_OPTIONS,
  principalCapWarning,
  principalLoadDefaults,
} from '../../utils/chiefRules'
import type { ChiefType, NewTeacher, NewTeacherProfile, TeacherWithCapacity, TermWithDates } from '../../types/models'
import ChangeDetailsDialog from '../history/ChangeDetailsDialog.vue'

const props = defineProps<{
  visible: boolean
  teacher: TeacherWithCapacity | null
  /** Mevcut kayıtlardan toplanan dal adları; öneri olarak sunulur. */
  knownBranches: string[]
  /** Aktif dönemin tarihleri; `null` iken henüz yüklenmemiştir, tarih/gerekçe sorulmaz. */
  term: TermWithDates | null
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  /** Yalnız yeni kayıt: yük alanları dahil, tarih/gerekçe ile. */
  save: [input: NewTeacher, details: { effectiveDate: string | null; reason: string | null }]
  /** Yalnız düzenleme: yalnız profil alanları; yük tarihçe yolundan değişir. */
  update: [profile: NewTeacherProfile]
}>()

function emptyForm(): NewTeacher {
  return {
    firstName: '',
    lastName: '',
    registryNo: '',
    field: 'Elektrik-Elektronik Teknolojisi',
    branches: [],
    employmentType: 'tenured',
    baseHours: 20,
    maxExtraHours: 24,
    otherExtraHours: 0,
    chiefType: 'none',
    isActive: true,
  }
}

const form = reactive<NewTeacher>(emptyForm())
const branchSuggestions = ref<string[]>([])
const isChangeDetailsOpen = ref(false)

const isEdit = computed(() => props.teacher !== null)

const employmentOptions = [
  { value: 'tenured' as const, label: labels.employmentType.tenured },
  { value: 'contracted' as const, label: labels.employmentType.contracted },
]

const chiefOptions = CHIEF_OPTIONS

const chiefHours = computed(() => CHIEF_HOURS_BY_TYPE[form.chiefType])

// Şeflik saati azamî ek ders tavanının İÇİNDEN düşer; tavan şeflikten küçükse
// kayıt tutarsızdır ve Rust tarafı da reddeder. Müdür/müdür yardımcısında ayrıca
// MADDE 6/1-a tavanı (6 saat) uygulanır.
const capacityWarning = computed<string | null>(() => {
  if (form.maxExtraHours < chiefHours.value) return labels.teacher.capacityWarning
  return principalCapWarning(form.chiefType, form.maxExtraHours)
})

/** Kullanıcı unvanı değiştirince müdür kadrosunun sabit saatleri forma yazılır. */
function onChiefTypeChange(value: ChiefType): void {
  form.chiefType = value
  const defaults = principalLoadDefaults(value)
  if (defaults) Object.assign(form, defaults)
}

const isValid = computed(
  () =>
    form.firstName.trim().length > 0 &&
    form.lastName.trim().length > 0 &&
    form.field.trim().length > 0 &&
    (isEdit.value || capacityWarning.value === null),
)

/** Yalnız yeni kayıtta, dönem başlamışsa yürürlük tarihi/gerekçe sorulur. */
const needsChangeDetails = computed(
  () => !isEdit.value && props.term !== null && !props.term.isPlanning,
)

// `ChangeDetailsDialog` `TermWithDates` zorunlu kılar; dönem henüz
// yüklenmediyse (`needsChangeDetails` zaten false olur) zararsız bir yer tutucu döner.
const changeDetailsTerm = computed<TermWithDates>(
  () =>
    props.term ?? {
      term: '',
      startDate: '',
      endDate: '',
      datesConfirmed: false,
      isPlanning: true,
      defaultAsOf: '',
      earliestAllowedDate: '',
    },
)

function onBranchComplete(event: { query: string }): void {
  const query = event.query.trim().toLowerCase()
  branchSuggestions.value = props.knownBranches.filter((branch) =>
    branch.toLowerCase().includes(query),
  )
}

watch(
  () => [props.visible, props.teacher] as const,
  ([isVisible, teacher]) => {
    if (!isVisible) return
    if (teacher) {
      Object.assign(form, {
        firstName: teacher.firstName,
        lastName: teacher.lastName,
        registryNo: teacher.registryNo,
        field: teacher.field,
        branches: parseBranches(teacher.branches),
        employmentType: teacher.employmentType,
        baseHours: teacher.baseHours,
        maxExtraHours: teacher.maxExtraHours,
        otherExtraHours: teacher.otherExtraHours,
        chiefType: teacher.chiefType,
        isActive: teacher.isActive === 1,
      })
    } else {
      Object.assign(form, emptyForm())
    }
    isChangeDetailsOpen.value = false
  },
  { immediate: true },
)

function close(): void {
  isChangeDetailsOpen.value = false
  emit('update:visible', false)
}

function emitSave(effectiveDate: string | null, reason: string | null): void {
  emit('save', { ...form, branches: [...form.branches] }, { effectiveDate, reason })
  close()
}

/** Formdan yalnız profil alanlarını alır (yük alanları dahil edilmez). */
function toProfile(): NewTeacherProfile {
  return {
    firstName: form.firstName,
    lastName: form.lastName,
    registryNo: form.registryNo,
    field: form.field,
    branches: [...form.branches],
    isActive: form.isActive,
  }
}

/** Düzenlemede yalnız profili yayar; yeni kayıtta dönem başlamışsa önce
 * yürürlük tarihi ve gerekçeyi soran pencereyi açar. */
function save(): void {
  if (isEdit.value) {
    emit('update', toProfile())
    close()
    return
  }
  if (needsChangeDetails.value) {
    isChangeDetailsOpen.value = true
    return
  }
  emitSave(null, null)
}

function onChangeDetailsConfirm(details: { effectiveDate: string | null; reason: string }): void {
  emitSave(details.effectiveDate, details.reason)
}
</script>

<style scoped>
.form-grid { display: flex; flex-direction: column; gap: 1rem; }
.field { display: flex; flex-direction: column; gap: 0.375rem; flex: 1; }
.field-row { display: flex; gap: 1rem; }
.field--inline { flex-direction: row; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; }
</style>
