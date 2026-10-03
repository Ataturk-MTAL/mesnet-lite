<template>
  <Dialog
    :visible="visible"
    modal
    :header="labels.allocation.proposalTitle"
    :style="{ width: '52rem' }"
    :breakpoints="{ '1100px': '92vw' }"
    @update:visible="(value: boolean) => emit('update:visible', value)"
  >
    <div class="mode-block">
      <span id="proposal-mode-label" class="mode-label">{{ labels.allocation.proposalMode }}</span>
      <SelectButton
        :model-value="mode"
        :options="modeOptions"
        option-label="label"
        option-value="value"
        option-disabled="disabled"
        :allow-empty="false"
        aria-labelledby="proposal-mode-label"
        @update:model-value="onModeChange"
      />
      <small class="muted">{{ modeHint }}</small>
      <Message v-if="!isPlanning" severity="info" :closable="false" class="mode-note">
        {{ labels.allocation.proposalRedistributeDisabled }}
      </Message>
    </div>

    <div v-if="isLoading" class="empty" role="status" aria-live="polite">
      {{ labels.allocation.proposalLoading }}
    </div>

    <template v-else-if="proposal">
      <div class="summary" data-testid="proposal-summary">
        <div class="summary-item">
          <div class="summary-value">{{ proposal.placedCount }}</div>
          <div class="summary-label">{{ labels.allocation.proposalPlacedCount }}</div>
        </div>
        <div class="summary-item">
          <div class="summary-value">{{ proposal.totalHours }}</div>
          <div class="summary-label">{{ labels.allocation.proposalTotalHours }}</div>
        </div>
        <div class="summary-item">
          <div class="summary-value" :class="{ 'summary-value--over': isPoolOverrun }">
            {{ proposal.poolRemaining ?? '—' }}
          </div>
          <div class="summary-label">
            {{ proposal.poolRemaining === null ? labels.allocation.proposalPoolUndefined : labels.allocation.proposalPoolRemaining }}
          </div>
        </div>
      </div>

      <Message v-for="(warning, index) in proposal.warnings" :key="index" severity="warn" :closable="false">
        {{ warning }}
      </Message>

      <Message v-if="noChanges" severity="secondary" :closable="false" data-testid="proposal-no-changes">
        {{ labels.allocation.proposalNoChanges }}
      </Message>

      <Panel
        v-if="proposal.assignments.length > 0"
        :header="`${labels.allocation.proposalAssignments} (${proposal.assignments.length})`"
        toggleable
        class="section"
        data-testid="proposal-assignments"
      >
        <div v-for="item in proposal.assignments" :key="item.companyId" class="row">
          <div class="row-main">
            <div class="company-name">{{ item.companyName }}</div>
            <div class="row-meta">
              <Tag :value="item.teacherName" severity="secondary" />
              <span class="muted">{{ slotLabel(item.visitDay, item.visitHour, item.visitEndHour) }}</span>
              <Tag
                :value="item.previous === null ? labels.allocation.proposalNew : labels.allocation.proposalMoved"
                :severity="item.previous === null ? 'success' : 'info'"
              />
              <Tag v-if="!item.exactBranchMatch" :value="labels.allocation.nearField" severity="warn" />
            </div>
            <div v-if="item.previous !== null" class="muted">
              {{ labels.allocation.proposalPreviousPlace }}:
              {{ teacherNames.get(item.previous.teacherId) ?? '' }}
              {{ previousSlotLabel(item.previous, item.previousHours) }}
            </div>
          </div>
        </div>
      </Panel>

      <Panel
        v-if="proposal.hourChanges.length > 0"
        :header="`${labels.allocation.proposalHourChanges} (${proposal.hourChanges.length})`"
        toggleable
        class="section"
        data-testid="proposal-hour-changes"
      >
        <div v-for="item in proposal.hourChanges" :key="item.companyId" class="row">
          <div class="row-main">
            <div class="company-name">{{ item.companyName }}</div>
            <div class="row-meta">
              <span class="hours-change" :class="{ 'hours-change--zero': item.newHours === 0 }">
                {{ item.oldHours }} → {{ item.newHours }} {{ labels.allocation.hoursSuffix }}
              </span>
              <Tag v-if="item.newHours === 0" :value="labels.allocation.proposalHonoraryBadge" severity="danger" />
            </div>
            <div class="muted">{{ item.reason }}</div>
          </div>
        </div>
      </Panel>

      <Panel
        v-if="proposal.released.length > 0"
        :header="`${labels.allocation.proposalReleased} (${proposal.released.length})`"
        toggleable
        class="section"
        data-testid="proposal-released"
      >
        <div v-for="item in proposal.released" :key="item.companyId" class="row">
          <div class="row-main">
            <div class="company-name">{{ item.companyName }}</div>
            <div class="muted">
              {{ labels.allocation.proposalReleasedFrom }}:
              {{ teacherNames.get(item.previous.teacherId) ?? '' }}
              {{ slotLabel(item.previous.visitDay, item.previous.visitHour, item.previous.visitHour) }}
            </div>
          </div>
        </div>
      </Panel>

      <Panel
        v-if="proposal.unassigned.length > 0"
        :header="`${labels.allocation.proposalUnassigned} (${proposal.unassigned.length})`"
        toggleable
        class="section"
        data-testid="proposal-unassigned"
      >
        <div v-for="item in proposal.unassigned" :key="item.companyId" class="row">
          <div class="row-main">
            <div class="row-meta">
              <span class="company-name">{{ item.companyName }}</span>
              <Tag v-if="item.wasAssigned" :value="labels.allocation.proposalWasAssigned" severity="warn" />
            </div>
            <div class="muted">{{ item.reason }}</div>
          </div>
        </div>
      </Panel>

      <Panel
        v-if="proposal.groupSplits.length > 0"
        :header="`${labels.allocation.proposalGroupSplits} (${proposal.groupSplits.length})`"
        toggleable
        class="section"
        data-testid="proposal-group-splits"
      >
        <div v-for="split in proposal.groupSplits" :key="split.teacherId" class="row">
          <div class="row-main">
            <Tag :value="split.teacherName" severity="secondary" class="split-teacher" />
            <ul class="split-groups">
              <li v-for="(group, index) in split.groups" :key="group.groupKey ?? `none-${index}`">
                <strong>{{ group.groupLabel || labels.allocation.proposalUngrouped }}</strong>:
                <span class="muted">{{ group.companyNames.join(', ') }}</span>
              </li>
            </ul>
          </div>
        </div>
      </Panel>

      <Panel
        v-if="proposal.teacherLoads.length > 0"
        :header="labels.allocation.proposalTeacherLoads"
        toggleable
        class="section"
        data-testid="proposal-teacher-loads"
      >
        <DataTable :value="proposal.teacherLoads" data-key="teacherId" size="small" stripedRows>
          <Column field="teacherName" :header="labels.allocation.proposalColTeacher" />
          <Column :header="labels.allocation.proposalColHours">
            <template #body="{ data }">
              <span :class="{ over: data.hours > data.capacity }">{{ data.hours }} / {{ data.capacity }}</span>
            </template>
          </Column>
          <Column field="companyCount" :header="labels.allocation.proposalColCompanies" />
          <Column field="distinctGroups" :header="labels.allocation.proposalColGroups" />
        </DataTable>
      </Panel>
    </template>

    <Message v-if="rejection !== null" severity="error" :closable="false" class="rejection">
      <div>{{ rejection }}</div>
      <div class="muted">{{ labels.allocation.proposalRejectedNote }}</div>
    </Message>

    <template #footer>
      <Button
        :label="labels.common.cancel"
        severity="secondary"
        outlined
        @click="emit('update:visible', false)"
      />
      <Button
        :label="labels.allocation.proposalApply"
        :aria-label="labels.allocation.proposalApply"
        :loading="isApplying"
        :disabled="!canApply"
        @click="onApply"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { assignmentsApi } from '../../api/assignments'
import type { AllocationProposal, ProposalMode } from '../../api/assignments'
import { labels } from '../../i18n/labels'
import { hasNoChanges } from '../../utils/proposalApply'
import { previousSlotLabel, slotLabel, teacherNameMap } from '../../utils/proposalFormat'

const props = defineProps<{
  visible: boolean
  /** Dönem planlamadaysa true; başladıysa "Baştan dağıt" kapalı. */
  isPlanning: boolean
  isApplying: boolean
  /** Arka uç uygulamayı reddettiyse gerekçe; hiçbir şey yazılmamıştır. */
  rejection: string | null
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  apply: [proposal: AllocationProposal]
  /** Öneri yeniden istendi; üst bileşen eski reddi temizlemeli. */
  proposalChange: []
}>()

const toast = useToast()
const DEFAULT_MODE: ProposalMode = 'fillGaps'

// Yalnız bu pencereye ait geçici durum.
const mode = ref<ProposalMode>(DEFAULT_MODE)
const proposal = ref<AllocationProposal | null>(null)
const isLoading = ref(false)
/** Hızlı kip değişiminde geç gelen eski yanıtı yok saymak için artan sayaç. */
let requestSeq = 0

const modeOptions = computed(() => [
  { value: 'fillGaps' as const, label: labels.allocation.proposalModeFillGaps, disabled: false },
  {
    value: 'redistribute' as const,
    label: labels.allocation.proposalModeRedistribute,
    disabled: !props.isPlanning,
  },
])

const modeHint = computed(() =>
  mode.value === 'fillGaps'
    ? labels.allocation.proposalModeFillGapsHint
    : labels.allocation.proposalModeRedistributeHint,
)

const noChanges = computed(() => proposal.value !== null && hasNoChanges(proposal.value))
const isPoolOverrun = computed(() => (proposal.value?.poolRemaining ?? 0) < 0)
const teacherNames = computed(() => (proposal.value ? teacherNameMap(proposal.value) : new Map<number, string>()))
const canApply = computed(
  () => proposal.value !== null && !noChanges.value && !isLoading.value && !props.isApplying,
)

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

/** Öneriyi geçerli kiple ister; hiçbir şey kaydetmez. Hata sessiz yutulmaz. */
async function loadProposal(): Promise<void> {
  const seq = ++requestSeq
  isLoading.value = true
  proposal.value = null
  emit('proposalChange')
  try {
    const result = await assignmentsApi.propose(mode.value)
    if (seq === requestSeq) proposal.value = result
  } catch (error: unknown) {
    if (seq !== requestSeq) return
    showError(error)
    // İlk istek başarısızsa açık bir boş pencere bırakma.
    if (mode.value === DEFAULT_MODE) emit('update:visible', false)
  } finally {
    if (seq === requestSeq) isLoading.value = false
  }
}

function onModeChange(value: ProposalMode | null): void {
  if (value === null || value === mode.value) return
  mode.value = value
  void loadProposal()
}

function onApply(): void {
  if (proposal.value !== null && canApply.value) emit('apply', proposal.value)
}

watch(
  () => props.visible,
  (isVisible) => {
    if (isVisible) {
      mode.value = DEFAULT_MODE
      void loadProposal()
    } else {
      requestSeq += 1
      proposal.value = null
      isLoading.value = false
    }
  },
  { immediate: true },
)
</script>

<style scoped>
.mode-block { display: flex; flex-direction: column; gap: 0.5rem; margin-bottom: 1rem; align-items: flex-start; }
.mode-label { font-size: 0.875rem; font-weight: 500; }
.mode-note { align-self: stretch; }
.muted { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.empty { color: var(--p-text-muted-color); padding: 1rem 0; text-align: center; }
.over { color: var(--p-red-500); font-weight: 600; }
.summary { display: flex; flex-wrap: wrap; gap: 1.5rem; margin-bottom: 1rem; }
.summary-item { min-width: 8rem; }
.summary-value { font-size: 1.5rem; font-weight: 600; }
.summary-value--over { color: var(--p-red-500); }
.summary-label { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.section { margin-top: 1rem; }
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.5rem 0;
  border-bottom: 1px solid var(--p-content-border-color);
}
.row:last-child { border-bottom: none; }
.row-main { min-width: 0; display: flex; flex-direction: column; gap: 0.25rem; }
.row-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
.company-name { font-weight: 600; overflow-wrap: anywhere; }
.hours-change { font-variant-numeric: tabular-nums; }
.hours-change--zero { color: var(--p-red-500); font-weight: 600; }
.split-teacher { align-self: flex-start; }
.split-groups { margin: 0.25rem 0 0; padding-left: 1.25rem; }
.rejection { margin-top: 1rem; }
</style>
