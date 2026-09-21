<template>
  <Dialog
    :visible="visible"
    @update:visible="onVisibleChange"
    modal
    :header="labels.companyMerge.title"
    :style="{ width: '34rem' }"
    data-testid="company-merge-dialog"
  >
    <div class="merge-body" v-if="source">
      <div class="merge-field">
        <span class="merge-label">{{ labels.companyMerge.sourceLabel }}</span>
        <p class="merge-source-name">{{ source.name }}</p>
      </div>

      <div class="merge-field">
        <label :for="targetSelectId">{{ labels.companyMerge.targetLabel }}</label>
        <Select
          :id="targetSelectId"
          v-model="selectedTargetId"
          :options="targetOptions"
          optionLabel="name"
          optionValue="id"
          filter
          :filterFields="['name']"
          :placeholder="labels.companyMerge.targetPlaceholder"
          :filterPlaceholder="labels.companyMerge.targetSearchPlaceholder"
          :ariaLabel="labels.companyMerge.targetLabel"
          fluid
          data-testid="company-merge-target-select"
        />
      </div>

      <Message v-if="previewStatus === 'loading'" severity="secondary" :closable="false">
        {{ labels.companyMerge.previewLoading }}
      </Message>

      <Message v-if="previewStatus === 'error'" severity="error" :closable="false" data-testid="company-merge-preview-error">
        {{ previewErrorMessage }}
      </Message>

      <p v-if="previewStatus === 'idle'" class="merge-hint">{{ labels.companyMerge.selectTargetHint }}</p>

      <template v-if="previewStatus === 'loaded' && preview">
        <Message severity="warn" :closable="false" data-testid="company-merge-direction">
          {{ labels.companyMerge.direction(preview.fromName, preview.intoName) }}
        </Message>

        <section class="merge-section">
          <h3>{{ labels.companyMerge.studentsSection }}</h3>
          <ul v-if="preview.students.length > 0" class="merge-list">
            <li v-for="student in preview.students" :key="student.studentId">{{ student.fullName }}</li>
          </ul>
          <p v-else class="merge-empty-note">{{ labels.companyMerge.noStudents }}</p>
        </section>

        <p class="merge-detail">
          {{ labels.companyMerge.awardedHoursToClear }}: {{ preview.awardedHoursToClear }}
        </p>
        <p class="merge-detail-note">{{ labels.companyMerge.awardedHoursNote }}</p>

        <p class="merge-detail">
          {{ preview.endsCoordination ? labels.companyMerge.endsCoordinationYes : labels.companyMerge.endsCoordinationNo }}
        </p>

        <section class="merge-section">
          <h3>{{ labels.companyMerge.warningsSection }}</h3>
          <ul v-if="preview.warnings.length > 0" class="merge-list">
            <li v-for="(warning, index) in preview.warnings" :key="index">{{ warning }}</li>
          </ul>
          <p v-else class="merge-empty-note">{{ labels.companyMerge.noWarnings }}</p>
        </section>
      </template>
    </div>

    <template #footer>
      <Button :label="labels.common.cancel" severity="secondary" outlined @click="close" />
      <Button
        :label="labels.companyMerge.confirmButton"
        severity="danger"
        :disabled="previewStatus !== 'loaded'"
        data-testid="company-merge-confirm-button"
        @click="openChangeDetails"
      />
    </template>
  </Dialog>

  <!-- Birleştirme kalıcı ve yıkıcıdır; yürürlük tarihi ve gerekçe bu pencereyle sorulur. -->
  <ChangeDetailsDialog
    :visible="isChangeDetailsOpen"
    :term="changeDetailsTerm"
    :title="labels.companyMerge.changeDetailsTitle"
    :effective-date="null"
    reason=""
    @confirm="onChangeDetailsConfirm"
    @cancel="isChangeDetailsOpen = false"
  />
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { labels } from '../../i18n/labels'
import { companiesApi } from '../../api/companies'
import ChangeDetailsDialog from '../history/ChangeDetailsDialog.vue'
import type { CompanyMergeInput, CompanyMergePreview } from '../../api/companies'
import type { Company, TermWithDates } from '../../types/models'

const props = defineProps<{
  visible: boolean
  /** `null` iken pencere içeriği gösterilmez; kaynak seçimi üst ekranda yapılır. */
  source: Company | null
  /** Tüm işletmeler; kaynağın kendisi burada dışlanır (aşağıdaki `targetOptions`). */
  companies: Company[]
  /** Aktif dönemin tarihleri; yürürlük tarihi/gerekçe penceresi bunu kullanır. */
  term: TermWithDates | null
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  confirm: [payload: CompanyMergeInput]
}>()

type PreviewStatus = 'idle' | 'loading' | 'loaded' | 'error'

const targetSelectId = useId()
const selectedTargetId = ref<number | null>(null)
const preview = ref<CompanyMergePreview | null>(null)
const previewStatus = ref<PreviewStatus>('idle')
const previewErrorMessage = ref<string | null>(null)
const isChangeDetailsOpen = ref(false)

// NOT: Company tipinde henüz bir "aktif/pasif" alanı yok (bkz. `list_companies`),
// bu yüzden burada yalnız kaynağın kendisi dışlanabiliyor. Daha önce birleştirilip
// pasifleşmiş bir işletme de bu listede görünmeye devam eder — bu, arka ucun
// `Company`/`list_companies` üzerinden `isActive` alanı sunması gereken bilinen bir eksiktir.
const targetOptions = computed(() => props.companies.filter((c) => c.id !== props.source?.id))

// ChangeDetailsDialog `TermWithDates` zorunlu kılar; dönem henüz yüklenmediyse
// "Birleştir" düğmesi zaten önizleme olmadan devre dışı kalır, bu yalnız zararsız bir yer tutucudur.
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

function resetPreview(): void {
  preview.value = null
  previewStatus.value = 'idle'
  previewErrorMessage.value = null
}

async function loadPreview(targetId: number): Promise<void> {
  const source = props.source
  if (!source) return
  previewStatus.value = 'loading'
  previewErrorMessage.value = null
  try {
    preview.value = await companiesApi.previewMerge(source.id, targetId)
    previewStatus.value = 'loaded'
  } catch (error: unknown) {
    preview.value = null
    previewErrorMessage.value = error instanceof Error ? error.message : labels.common.error
    previewStatus.value = 'error'
  }
}

watch(selectedTargetId, (targetId) => {
  if (targetId === null) {
    resetPreview()
    return
  }
  void loadPreview(targetId)
})

// Pencere her açılışta temiz başlar; kaynak değişirse de önceki seçim/önizleme geçersizdir.
watch(
  () => [props.visible, props.source?.id ?? null] as const,
  ([isVisible]) => {
    if (!isVisible) return
    selectedTargetId.value = null
    resetPreview()
    isChangeDetailsOpen.value = false
  },
  { immediate: true },
)

function close(): void {
  isChangeDetailsOpen.value = false
  emit('update:visible', false)
}

function onVisibleChange(next: boolean): void {
  if (!next) close()
}

function openChangeDetails(): void {
  if (previewStatus.value !== 'loaded') return
  isChangeDetailsOpen.value = true
}

function onChangeDetailsConfirm(details: { effectiveDate: string | null; reason: string }): void {
  const source = props.source
  const targetId = selectedTargetId.value
  if (!source || targetId === null) return
  emit('confirm', {
    fromCompanyId: source.id,
    intoCompanyId: targetId,
    effectiveDate: details.effectiveDate,
    reason: details.reason,
  })
  close()
}
</script>

<style scoped>
.merge-body { display: flex; flex-direction: column; gap: 1rem; }
.merge-field { display: flex; flex-direction: column; gap: 0.375rem; }
.merge-label { font-size: 0.875rem; font-weight: 500; }
.merge-field label { font-size: 0.875rem; font-weight: 500; }
.merge-source-name { margin: 0; font-weight: 600; }
.merge-hint { color: var(--p-text-muted-color); font-size: 0.875rem; margin: 0; }
.merge-section h3 { margin: 0 0 0.5rem; font-size: 0.9375rem; }
.merge-list { list-style: disc; margin: 0; padding-left: 1.25rem; display: flex; flex-direction: column; gap: 0.25rem; }
.merge-detail { margin: 0; font-size: 0.875rem; }
.merge-detail-note { margin: 0.125rem 0 0; color: var(--p-text-muted-color); font-size: 0.8125rem; }
.merge-empty-note { color: var(--p-text-muted-color); font-size: 0.875rem; margin: 0; }
</style>
