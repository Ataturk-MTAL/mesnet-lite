<template>
  <Dialog
    :visible="visible"
    @update:visible="emit('update:visible', $event)"
    modal
    :header="isEdit ? labels.common.edit : labels.common.add"
    :style="{ width: '32rem' }"
  >
    <div class="form-grid">
      <div class="field-row">
        <div class="field">
          <label for="student-first">{{ labels.student.firstName }}</label>
          <InputText id="student-first" v-model="form.firstName" autofocus />
        </div>
        <div class="field">
          <label for="student-last">{{ labels.student.lastName }}</label>
          <InputText id="student-last" v-model="form.lastName" />
        </div>
      </div>

      <div class="field-row">
        <div class="field">
          <label for="student-no">{{ labels.student.studentNo }}</label>
          <InputText id="student-no" v-model="studentNo" />
          <small class="hint">{{ labels.student.studentNoHint }}</small>
        </div>
        <div class="field">
          <label for="student-grade">{{ labels.student.grade }}</label>
          <InputText id="student-grade" v-model="form.grade" placeholder="12/C" />
        </div>
      </div>

      <div class="field">
        <label for="student-branch">{{ labels.student.branch }}</label>
        <InputText id="student-branch" v-model="form.branch" />
      </div>

      <div v-if="!isEdit" class="field">
        <label for="student-company">{{ labels.student.company }}</label>
        <Select
          id="student-company"
          v-model="form.companyId"
          :options="companyOptions"
          optionLabel="label"
          optionValue="value"
          :placeholder="labels.student.noCompany"
          showClear
          filter
        />
      </div>
    </div>

    <template #footer>
      <Button :label="labels.common.cancel" severity="secondary" outlined @click="close" />
      <Button :label="labels.common.save" :disabled="!isValid" @click="save" />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { labels } from '../../i18n/labels'
import type { Company, NewStudent, Student } from '../../types/models'

const props = defineProps<{
  visible: boolean
  student: Student | null
  companies: Company[]
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [input: NewStudent]
}>()

function emptyForm(): NewStudent {
  return {
    firstName: '',
    lastName: '',
    studentNo: null,
    grade: '',
    branch: '',
    companyId: null,
    submittedAt: null,
    // Boş bırakılır; backend aktif dönemi yazar.
    term: '',
  }
}

const form = reactive<NewStudent>(emptyForm())

// Boş metin ile null'u ayırmak için ara değişken: InputText boş string verir,
// veritabanı ise "numara yok" durumunu null olarak tutar.
const studentNo = ref('')

const isEdit = computed(() => props.student !== null)
const isValid = computed(
  () =>
    form.firstName.trim().length > 0 &&
    form.lastName.trim().length > 0 &&
    form.grade.trim().length > 0 &&
    form.branch.trim().length > 0,
)

const companyOptions = computed(() =>
  props.companies.map((company) => ({ value: company.id, label: company.name })),
)

watch(
  () => [props.visible, props.student] as const,
  ([isVisible, student]) => {
    if (!isVisible) return
    if (student) {
      Object.assign(form, {
        firstName: student.firstName,
        lastName: student.lastName,
        studentNo: student.studentNo,
        grade: student.grade,
        branch: student.branch,
        companyId: student.companyId,
        submittedAt: student.submittedAt,
        term: student.term,
      })
      studentNo.value = student.studentNo ?? ''
    } else {
      Object.assign(form, emptyForm())
      studentNo.value = ''
    }
  },
  { immediate: true },
)

function close(): void {
  emit('update:visible', false)
}

function save(): void {
  const trimmed = studentNo.value.trim()
  emit('save', { ...form, studentNo: trimmed.length > 0 ? trimmed : null })
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
