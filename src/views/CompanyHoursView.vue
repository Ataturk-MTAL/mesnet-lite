<template>
  <div class="page">
    <div class="page-header">
      <div class="title-group">
        <h1 class="page-title">{{ labels.hours.title }}</h1>
        <Tag v-if="board?.term" :value="board.term" severity="secondary" icon="pi pi-calendar" />
      </div>
      <div class="header-actions">
        <Button
          :label="labels.hours.autoDistribute"
          icon="pi pi-sparkles"
          severity="secondary"
          outlined
          :disabled="rows.length === 0"
          v-tooltip.bottom="labels.hours.autoDistributeTooltip"
          @click="runAutoDistribute"
        />
        <Button
          v-if="hasSuggestion"
          :label="labels.hours.undo"
          icon="pi pi-undo"
          severity="secondary"
          outlined
          @click="undoSuggestion"
        />
        <Button
          :label="labels.hours.save"
          icon="pi pi-check"
          :badge="changedCount > 0 ? String(changedCount) : undefined"
          :disabled="changedCount === 0"
          :loading="isSaving"
          @click="save"
        />
      </div>
    </div>

    <Message severity="secondary" :closable="false">{{ labels.hours.subtitle }}</Message>

    <Message
      v-for="(warning, index) in allWarnings"
      :key="index"
      :severity="warningSeverity(warning)"
      :closable="false"
    >
      {{ warning }}
    </Message>

    <!-- Havuz özeti -->
    <Card>
      <template #content>
        <div class="summary">
          <div class="summary-item">
            <div class="summary-value">{{ board?.poolHours ?? 0 }}</div>
            <div class="summary-label">{{ labels.hours.pool }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value">{{ liveTotalAwarded }}</div>
            <div class="summary-label">{{ labels.hours.totalAwarded }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value" :class="{ 'summary-value--over': isOverPool }">
              {{ (board?.poolHours ?? 0) - liveTotalAwarded }}
            </div>
            <div class="summary-label">{{ labels.hours.remaining }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value">{{ board?.totalMax ?? 0 }}</div>
            <div class="summary-label">{{ labels.hours.totalMax }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value">{{ liveHonoraryCount }}</div>
            <div class="summary-label">{{ labels.hours.honorary }}</div>
          </div>
        </div>
      </template>
    </Card>

    <DataTable
      :value="rows"
      :loading="isLoading"
      dataKey="companyId"
      paginator
      :rows="20"
      stripedRows
      v-model:filters="filters"
      :globalFilterFields="['companyName', 'addressText']"
    >
      <template #header>
        <InputText v-model="filters.global.value" :placeholder="labels.company.searchPlaceholder" />
      </template>
      <template #empty>{{ labels.hours.empty }}</template>

      <Column field="companyName" :header="labels.hours.company" sortable />

      <Column :header="labels.hours.distance" sortable field="roundTripDistanceKm">
        <template #body="{ data }">
          <span v-if="data.roundTripDistanceKm !== null">
            {{ data.roundTripDistanceKm.toFixed(1) }}
            <small class="muted">({{ data.oneWayDistanceKm?.toFixed(1) }} tek yön)</small>
          </span>
          <span v-else class="muted">—</span>
        </template>
      </Column>

      <Column field="studentCount" :header="labels.hours.students" sortable />

      <Column :header="labels.hours.maxHours" sortable field="maxHours">
        <template #body="{ data }">
          <strong v-if="data.maxHours !== null">{{ data.maxHours }}</strong>
          <Tag v-else :value="labels.hours.noRule" severity="warn" />
        </template>
      </Column>

      <Column :header="labels.hours.honorary">
        <template #body="{ data }">
          <ToggleSwitch
            :model-value="data.isHonorary"
            v-tooltip.top="labels.hours.honoraryTooltip"
            @update:model-value="(value: boolean) => setHonorary(data, value)"
          />
        </template>
      </Column>

      <Column :header="labels.hours.awarded">
        <template #body="{ data }">
          <Tag v-if="data.isHonorary" :value="labels.hours.honoraryBadge" severity="info" />
          <InputNumber
            v-else
            :model-value="data.awardedHours"
            :min="0"
            :max="data.maxHours ?? 0"
            :disabled="data.maxHours === null"
            showButtons
            buttonLayout="horizontal"
            class="hours-input"
            @update:model-value="(value: number | null) => setAwarded(data, value)"
          />
        </template>
      </Column>

      <Column :header="labels.hours.locked">
        <template #body="{ data }">
          <Button
            :icon="data.isLocked ? 'pi pi-lock' : 'pi pi-lock-open'"
            :severity="data.isLocked ? 'warn' : 'secondary'"
            outlined
            size="small"
            v-tooltip.top="labels.hours.lockedTooltip"
            :aria-label="labels.hours.locked"
            @click="toggleLock(data)"
          />
        </template>
      </Column>
    </DataTable>

    <div class="footer-actions">
      <span v-if="board && board.lockedCount > 0" class="muted">
        {{ board.lockedCount }} {{ labels.hours.lockedNote }}
      </span>
      <span class="muted">{{ labels.hours.savedHint }}</span>
      <RouterLink to="/allocation">
        <Button
          :label="labels.hours.goToAllocation"
          icon="pi pi-arrow-right"
          severity="secondary"
          outlined
        />
      </RouterLink>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { hoursApi } from '../api/hours'
import type { AutoDistributeRow, HoursBoard, HoursInput, HoursRow } from '../api/hours'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'

const toast = useToast()

const board = ref<HoursBoard | null>(null)
/** Ekranda düzenlenen kopyalar; kaydedilene kadar sunucuya gitmez. */
const rows = ref<HoursRow[]>([])
/** Otomatik dağıtım öncesi durum — `Geri Al` için. */
const snapshot = ref<HoursRow[] | null>(null)
const suggestionWarnings = ref<string[]>([])

const isLoading = ref(false)
const isSaving = ref(false)
const filters = ref({ global: { value: null as string | null, matchMode: 'contains' } })

const hasSuggestion = computed(() => snapshot.value !== null)

/** Kaydedilmemiş satır sayısı. */
const changedCount = computed(() => {
  const original = board.value?.rows ?? []
  return rows.value.filter((row) => {
    const source = original.find((r) => r.companyId === row.companyId)
    if (!source) return true
    return (
      source.awardedHours !== row.awardedHours ||
      source.isHonorary !== row.isHonorary ||
      source.isLocked !== row.isLocked
    )
  }).length
})

// Toplamlar kaydetmeden önce de canlı görünmeli.
const liveTotalAwarded = computed(() =>
  rows.value.reduce((sum, row) => sum + row.awardedHours, 0),
)
const liveHonoraryCount = computed(() => rows.value.filter((row) => row.isHonorary).length)
const isOverPool = computed(
  () => (board.value?.poolHours ?? 0) > 0 && liveTotalAwarded.value > (board.value?.poolHours ?? 0),
)

const allWarnings = computed(() => [...(board.value?.warnings ?? []), ...suggestionWarnings.value])

function warningSeverity(warning: string): string {
  return warning.includes('aşıyor') || warning.includes('aşıldı') ? 'error' : 'warn'
}

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

/** Fahri açılınca saat 0'a düşer; kapanınca kullanıcı yeniden girer. */
function setHonorary(row: HoursRow, value: boolean): void {
  row.isHonorary = value
  if (value) row.awardedHours = 0
}

function setAwarded(row: HoursRow, value: number | null): void {
  const requested = value ?? 0
  const cap = row.maxHours ?? 0
  if (requested > cap) {
    toast.add({ severity: 'warn', summary: labels.hours.exceedsMax, life: 4000 })
    row.awardedHours = cap
    return
  }
  row.awardedHours = Math.max(0, requested)
}

function toggleLock(row: HoursRow): void {
  row.isLocked = !row.isLocked
}

function applyBoard(next: HoursBoard): void {
  board.value = next
  // Derin kopya: düzenlemeler kaynak veriyi bozmamalı, yoksa changedCount
  // her zaman 0 çıkar.
  rows.value = next.rows.map((row) => ({ ...row }))
  snapshot.value = null
  suggestionWarnings.value = []
}

async function load(): Promise<void> {
  isLoading.value = true
  try {
    applyBoard(await hoursApi.get())
  } catch (error: unknown) {
    showError(error)
  } finally {
    isLoading.value = false
  }
}

async function runAutoDistribute(): Promise<void> {
  // Öneri öncesi durum saklanır ki `Geri Al` çalışsın.
  snapshot.value = rows.value.map((row) => ({ ...row }))

  const payload: AutoDistributeRow[] = rows.value.map((row) => ({
    companyId: row.companyId,
    maxHours: row.maxHours ?? 0,
    studentCount: row.studentCount,
    isLocked: row.isLocked,
    currentAwarded: row.awardedHours,
    isHonorary: row.isHonorary,
  }))

  try {
    const outcome = await hoursApi.autoDistribute(payload)
    for (const result of outcome.results) {
      const row = rows.value.find((r) => r.companyId === result.companyId)
      if (!row) continue
      row.awardedHours = result.awardedHours
      row.isHonorary = result.isHonorary
    }
    suggestionWarnings.value = outcome.warnings
    toast.add({
      severity: 'info',
      summary: labels.hours.autoDistribute,
      detail: `${outcome.distributedHours} saat dağıtıldı`,
      life: 4000,
    })
  } catch (error: unknown) {
    snapshot.value = null
    showError(error)
  }
}

function undoSuggestion(): void {
  if (!snapshot.value) return
  rows.value = snapshot.value.map((row) => ({ ...row }))
  snapshot.value = null
  suggestionWarnings.value = []
}

async function save(): Promise<void> {
  isSaving.value = true
  try {
    const payload: HoursInput[] = rows.value.map((row) => ({
      companyId: row.companyId,
      maxHoursSnapshot: row.maxHours ?? 0,
      awardedHours: row.awardedHours,
      isHonorary: row.isHonorary,
      isLocked: row.isLocked,
      notes: row.notes,
    }))
    applyBoard(await hoursApi.save(payload))
    toast.add({ severity: 'success', summary: labels.common.saved, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  } finally {
    isSaving.value = false
  }
}

// Dönem değişince takdirler de değişir.
watch(activeTerm, load)

onMounted(load)
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}
.title-group { display: flex; align-items: center; gap: 0.75rem; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }
.header-actions { display: flex; gap: 0.5rem; flex-wrap: wrap; }

.summary { display: flex; flex-wrap: wrap; gap: 2.5rem; }
.summary-item { min-width: 7rem; }
.summary-value { font-size: 1.5rem; font-weight: 700; line-height: 1.1; }
.summary-value--over { color: var(--p-red-500); }
.summary-label { font-size: 0.8125rem; color: var(--p-text-muted-color); margin-top: 0.125rem; }

.hours-input { width: 9rem; }
.muted { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.footer-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 1rem;
  flex-wrap: wrap;
}
</style>
