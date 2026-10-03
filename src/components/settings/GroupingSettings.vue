<template>
  <Card>
    <template #title>{{ labels.settings.grouping.title }}</template>
    <template #content>
      <p class="description">{{ labels.settings.grouping.description }}</p>

      <div
        class="mode"
        role="radiogroup"
        :aria-label="labels.settings.grouping.mode"
      >
        <div class="mode-option">
          <RadioButton
            input-id="grouping-mode-distance"
            name="grouping-mode"
            value="distance"
            :model-value="modelValue.mode"
            @update:model-value="setMode"
          />
          <label for="grouping-mode-distance">{{ labels.settings.grouping.modeDistance }}</label>
        </div>
        <div class="mode-option">
          <RadioButton
            input-id="grouping-mode-manual"
            name="grouping-mode"
            value="manual"
            :model-value="modelValue.mode"
            @update:model-value="setMode"
          />
          <label for="grouping-mode-manual">{{ labels.settings.grouping.modeManual }}</label>
        </div>
      </div>

      <div v-if="modelValue.mode === 'distance'" class="field">
        <label for="grouping-max-diameter">{{ labels.settings.grouping.maxDiameter }}</label>
        <InputNumber
          input-id="grouping-max-diameter"
          :model-value="modelValue.maxDiameterKm"
          :min="GROUPING_MIN_DIAMETER_KM"
          :max="GROUPING_MAX_DIAMETER_KM"
          :step="GROUPING_DIAMETER_STEP_KM"
          :min-fraction-digits="0"
          :max-fraction-digits="1"
          locale="tr-TR"
          show-buttons
          fluid
          :invalid="!validation.isDiameterValid"
          :aria-label="labels.settings.grouping.maxDiameter"
          @update:model-value="setDiameter"
        />
        <small class="hint" :class="{ 'hint--error': !validation.isDiameterValid }">
          {{
            validation.isDiameterValid
              ? labels.settings.grouping.maxDiameterHint
              : labels.settings.grouping.maxDiameterInvalid
          }}
        </small>
      </div>

      <div v-else class="manual">
        <p class="hint">{{ labels.settings.grouping.groupsHint }}</p>

        <p v-if="modelValue.groups.length === 0" class="empty">
          {{ labels.settings.grouping.noGroups }}
        </p>

        <div
          v-for="(group, index) in modelValue.groups"
          :key="index"
          class="group"
          role="group"
          :aria-label="labels.settings.grouping.groupTitle(index + 1)"
        >
          <div class="group-head">
            <div class="field group-name">
              <label :for="`grouping-name-${index}`">{{ labels.settings.grouping.groupName }}</label>
              <InputText
                :id="`grouping-name-${index}`"
                :model-value="group.name"
                :invalid="validation.nameErrors[index] !== null"
                :aria-describedby="`grouping-name-error-${index}`"
                fluid
                @update:model-value="(value) => updateGroup(index, { name: String(value ?? '') })"
              />
              <small
                v-if="validation.nameErrors[index] !== null"
                :id="`grouping-name-error-${index}`"
                class="hint hint--error"
                role="alert"
              >
                {{ nameErrorText(validation.nameErrors[index]) }}
              </small>
            </div>
            <Button
              icon="pi pi-trash"
              severity="danger"
              outlined
              size="small"
              :aria-label="labels.settings.grouping.removeGroup"
              v-tooltip.top="labels.settings.grouping.removeGroup"
              @click="removeGroup(index)"
            />
          </div>

          <div class="group-values">
            <div class="field">
              <label :for="`grouping-neighborhoods-${index}`">{{ labels.settings.grouping.neighborhoods }}</label>
              <AutoComplete
                :input-id="`grouping-neighborhoods-${index}`"
                :model-value="group.neighborhoods"
                :suggestions="neighborhoodSuggestions"
                multiple
                dropdown
                fluid
                :aria-label="labels.settings.grouping.neighborhoods"
                @complete="(event) => completeNeighborhoods(event.query)"
                @update:model-value="(value) => updateGroup(index, { neighborhoods: toValues(value) })"
              />
            </div>
            <div class="field">
              <label :for="`grouping-districts-${index}`">{{ labels.settings.grouping.districts }}</label>
              <AutoComplete
                :input-id="`grouping-districts-${index}`"
                :model-value="group.districts"
                :suggestions="districtSuggestions"
                multiple
                dropdown
                fluid
                :aria-label="labels.settings.grouping.districts"
                @complete="(event) => completeDistricts(event.query)"
                @update:model-value="(value) => updateGroup(index, { districts: toValues(value) })"
              />
            </div>
          </div>
        </div>

        <Button
          :label="labels.settings.grouping.addGroup"
          icon="pi pi-plus"
          severity="secondary"
          outlined
          size="small"
          @click="addGroup"
        />
      </div>
    </template>
  </Card>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { labels } from '../../i18n/labels'
import {
  GROUPING_DIAMETER_STEP_KM,
  GROUPING_MAX_DIAMETER_KM,
  GROUPING_MIN_DIAMETER_KM,
  uniqueValues,
  validateGrouping,
} from '../../utils/groupingSettings'
import type { GroupNameError, GroupingDraft, GroupingMode, ManualGroupDraft } from '../../utils/groupingSettings'

const props = defineProps<{
  modelValue: GroupingDraft
  /** Mevcut işletmelerin mahalleleri; yalnız öneri listesidir, serbest giriş de geçerlidir. */
  neighborhoodOptions: string[]
  districtOptions: string[]
}>()
const emit = defineEmits<{ 'update:modelValue': [value: GroupingDraft] }>()

const validation = computed(() => validateGrouping(props.modelValue))

const neighborhoodSuggestions = ref<string[]>([])
const districtSuggestions = ref<string[]>([])

function setMode(value: unknown): void {
  const mode: GroupingMode = value === 'manual' ? 'manual' : 'distance'
  emit('update:modelValue', { ...props.modelValue, mode })
}

function setDiameter(value: number | null): void {
  emit('update:modelValue', { ...props.modelValue, maxDiameterKm: value })
}

function updateGroup(index: number, patch: Partial<ManualGroupDraft>): void {
  const groups = props.modelValue.groups.map((group, i) => (i === index ? { ...group, ...patch } : group))
  emit('update:modelValue', { ...props.modelValue, groups })
}

function addGroup(): void {
  const groups = [...props.modelValue.groups, { name: '', neighborhoods: [], districts: [] }]
  emit('update:modelValue', { ...props.modelValue, groups })
}

function removeGroup(index: number): void {
  const groups = props.modelValue.groups.filter((_, i) => i !== index)
  emit('update:modelValue', { ...props.modelValue, groups })
}

/** Çip girdisi `unknown` gelir; yalnız metinleri alır, yinelenenleri atar. */
function toValues(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return uniqueValues(value.filter((v): v is string => typeof v === 'string'))
}

/**
 * Öneri listesi: eşleşen mevcut değerler; yazılan değer listede yoksa en üstte
 * kendisi de önerilir, böylece listede olmayan bir değer de seçilerek eklenebilir.
 */
function suggest(options: string[], query: string): string[] {
  const needle = query.trim()
  if (needle.length === 0) return options
  const lowered = needle.toLocaleLowerCase('tr')
  const matches = options.filter((option) => option.toLocaleLowerCase('tr').includes(lowered))
  const hasExact = options.some((option) => option.toLocaleLowerCase('tr') === lowered)
  return hasExact ? matches : [needle, ...matches]
}

function completeNeighborhoods(query: string): void {
  neighborhoodSuggestions.value = suggest(props.neighborhoodOptions, query)
}

function completeDistricts(query: string): void {
  districtSuggestions.value = suggest(props.districtOptions, query)
}

function nameErrorText(error: GroupNameError | null): string {
  if (error === 'empty') return labels.settings.grouping.nameEmpty
  if (error === 'duplicate') return labels.settings.grouping.nameDuplicate
  if (error === 'tooLong') return labels.settings.grouping.nameTooLong
  return ''
}
</script>

<style scoped>
.description { margin: 0 0 1rem; color: var(--p-text-muted-color); font-size: 0.875rem; }
.mode { display: flex; flex-wrap: wrap; gap: 1.5rem; margin-bottom: 1rem; }
.mode-option { display: flex; align-items: center; gap: 0.5rem; }
label { font-size: 0.875rem; font-weight: 500; }
.field { display: flex; flex-direction: column; gap: 0.375rem; min-width: 0; flex: 1 1 14rem; }
.hint { color: var(--p-text-muted-color); font-size: 0.75rem; margin: 0; }
.hint--error { color: var(--p-red-500); }
.empty { color: var(--p-text-muted-color); margin: 0.5rem 0; }
.manual { display: flex; flex-direction: column; gap: 0.75rem; align-items: flex-start; }
.group {
  align-self: stretch;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0.75rem;
  border: 1px solid var(--p-content-border-color);
  border-radius: var(--p-content-border-radius);
}
.group-head { display: flex; gap: 0.5rem; align-items: flex-start; }
.group-name { max-width: 26rem; }
.group-head > :deep(.p-button) { margin-top: 1.5rem; flex: 0 0 auto; }
.group-values { display: flex; flex-wrap: wrap; gap: 1rem; }
:deep(.p-inputnumber) { max-width: 14rem; }
</style>
