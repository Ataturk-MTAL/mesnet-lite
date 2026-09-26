<template>
  <div class="page">
    <div class="page-header">
      <div class="title-group">
        <h1 class="page-title">{{ labels.hours.title }}</h1>
        <Tag v-if="board?.term" :value="board.term" severity="secondary" icon="pi pi-calendar" />
      </div>
      <div class="header-actions">
        <Button
          :label="allRowsLocked ? labels.hours.unlockAll : labels.hours.lockAll"
          :icon="allRowsLocked ? 'pi pi-lock-open' : 'pi pi-lock'"
          severity="secondary"
          outlined
          :disabled="rows.length === 0"
          v-tooltip.bottom="labels.hours.lockAllTooltip"
          @click="toggleAllLocks"
        />
        <Button
          :label="labels.hours.autoDistribute"
          icon="pi pi-sparkles"
          severity="secondary"
          outlined
          :disabled="rows.length === 0 || allRowsLocked"
          v-tooltip.bottom="autoDistributeTooltipText"
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

    <!-- Havuz tanımlı değilse uyarı Ders Yükü ekranına yönlendirir; o ekran
         olmadan havuz kalıcı olarak 0 kalıyordu. -->
    <div v-if="isPoolUndefined" class="pool-warning-action">
      <RouterLink to="/teaching-load">
        <Button
          :label="labels.hours.goToTeachingLoad"
          icon="pi pi-arrow-right"
          severity="warn"
          outlined
        />
      </RouterLink>
    </div>

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
      tableStyle="min-width: 56rem"
    >
      <template #header>
        <InputText v-model="companyHoursSearch" :placeholder="labels.company.searchPlaceholder" />
      </template>
      <template #empty>{{ labels.hours.empty }}</template>

      <Column
        field="companyName"
        :header="labels.hours.company"
        sortable
        headerStyle="min-width: 14rem; width: 30%"
      />

      <Column
        :header="labels.hours.distance"
        sortable
        field="roundTripDistanceKm"
        headerStyle="min-width: 11rem"
      >
        <template #body="{ data }">
          <span v-if="data.roundTripDistanceKm !== null" class="distance-cell">
            {{ data.roundTripDistanceKm.toFixed(1) }}
            <small class="muted">({{ data.oneWayDistanceKm?.toFixed(1) }} tek yön)</small>
          </span>
          <span v-else class="muted">—</span>
        </template>
      </Column>

      <Column
        field="studentCount"
        :header="labels.hours.students"
        sortable
        headerStyle="min-width: 5.5rem"
      />

      <Column
        :header="labels.hours.maxHours"
        sortable
        field="maxHours"
        headerStyle="min-width: 7.5rem"
      >
        <template #body="{ data }">
          <strong v-if="data.maxHours !== null">{{ data.maxHours }}</strong>
          <Tag v-else :value="labels.hours.noRule" severity="warn" />
        </template>
      </Column>

      <Column :header="labels.hours.honorary" headerStyle="min-width: 6rem">
        <template #body="{ data }">
          <ToggleSwitch
            :model-value="data.isHonorary"
            :disabled="data.isLocked"
            :aria-label="labels.hours.honorary"
            v-tooltip.top="labels.hours.honoraryTooltip"
            @update:model-value="(value: boolean) => setHonorary(data, value)"
          />
        </template>
      </Column>

      <Column :header="labels.hours.awarded" headerStyle="min-width: 8rem">
        <template #body="{ data }">
          <Tag v-if="data.isHonorary" :value="labels.hours.honoraryBadge" severity="info" />
          <div v-else class="hours-input">
            <InputNumber
              fluid
              :model-value="data.awardedHours"
              :min="0"
              :max="data.maxHours ?? 0"
              :disabled="data.maxHours === null || data.isLocked"
              showButtons
              :aria-label="labels.hours.awarded"
              @update:model-value="(value: number | null) => setAwarded(data, value)"
            />
          </div>
        </template>
      </Column>

      <Column :header="labels.hours.locked" headerStyle="min-width: 5rem">
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
      <span v-if="liveLockedCount > 0" class="muted">
        {{ liveLockedCount }} {{ labels.hours.lockedNote }}
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

    <ChangeDetailsDialog
      v-if="activeTermDates"
      :visible="isChangeDialogOpen"
      :term="activeTermDates"
      :title="labels.history.changeDetailsTitle"
      :effective-date="lastChangeEffectiveDate"
      reason=""
      @confirm="confirmChangeDetails"
      @cancel="cancelChangeDetails"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useToast } from 'openvue/usetoast'
import type { DataTableFilterMeta } from 'openvue/datatable'
import { hoursApi } from '../api/hours'
import type { AutoDistributeRow, HoursBoard, HoursInput, HoursRow } from '../api/hours'
import { labels } from '../i18n/labels'
import { useTermStore } from '../stores/term'
import { useSelectionStore } from '../stores/selection'
import { useChangeDetailsDialog } from '../composables/useChangeDetailsDialog'
import ChangeDetailsDialog from '../components/history/ChangeDetailsDialog.vue'
import { buildGlobalFilter, extractGlobalFilterValue } from '../utils/dataTableFilters'

const toast = useToast()
const selection = useSelectionStore()
const { companyHoursSearch } = storeToRefs(selection)
const { activeTerm, activeTermDates } = storeToRefs(useTermStore())

/** Dönem tarihleri henüz yüklenmediyse planlama evresi varsayılır — Kaydet
 *  o kısa aralıkta engellenmez; gerçek yasak arka uçtan gelir. */
const isPlanning = computed(() => activeTermDates.value?.isPlanning ?? true)
const {
  isOpen: isChangeDialogOpen,
  lastEffectiveDate: lastChangeEffectiveDate,
  requestDetails: requestChangeDetails,
  confirm: confirmChangeDetails,
  cancel: cancelChangeDetails,
} = useChangeDetailsDialog(() => isPlanning.value)

const board = ref<HoursBoard | null>(null)
/** Ekranda düzenlenen kopyalar; kaydedilene kadar sunucuya gitmez. */
const rows = ref<HoursRow[]>([])
/** Otomatik dağıtım öncesi durum — `Geri Al` için. */
const snapshot = ref<HoursRow[] | null>(null)
const suggestionWarnings = ref<string[]>([])

const isLoading = ref(false)
const isSaving = ref(false)
// DataTable'ın arama kutusu iki yönlü; store'daki `companyHoursSearch` ile
// senkron kalması için OKUNABİLİR + YAZILABİLİR computed olarak sunulur.
const filters = computed<DataTableFilterMeta>({
  get: () => buildGlobalFilter(companyHoursSearch.value),
  set: (next) => {
    companyHoursSearch.value = extractGlobalFilterValue(next)
  },
})

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
const liveLockedCount = computed(() => rows.value.filter((row) => row.isLocked).length)
const isOverPool = computed(
  () => (board.value?.poolHours ?? 0) > 0 && liveTotalAwarded.value > (board.value?.poolHours ?? 0),
)
// Rust tarafındaki "havuz tanımlanmamış" uyarısıyla AYNI koşul (bkz.
// hours_commands.rs::load_board) — havuz 0 olduğunda Ders Yükü ekranına
// giden bir bağlantı gösterilir.
const isPoolUndefined = computed(() => board.value !== null && board.value.poolHours === 0)

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
  // Kilitli satır donmuş kabul edilir; girişler devre dışı olsa da ikinci kat
  // koruma olarak burada da erken dönülür.
  if (row.isLocked) return
  row.isHonorary = value
  if (value) row.awardedHours = 0
}

function setAwarded(row: HoursRow, value: number | null): void {
  // Kilitli satır donmuş kabul edilir; girişler devre dışı olsa da ikinci kat
  // koruma olarak burada da erken dönülür.
  if (row.isLocked) return
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

/** Tüm satırlar zaten kilitliyse toplu düğme "Kilitleri Aç" olur. */
const allRowsLocked = computed(() => rows.value.length > 0 && rows.value.every((row) => row.isLocked))

/** Hepsi kilitliyken "Otomatik Dağıt" pasif olur; tooltip nedeni açıklar. */
const autoDistributeTooltipText = computed(() =>
  allRowsLocked.value ? labels.hours.autoDistributeAllLocked : labels.hours.autoDistributeTooltip,
)

/** Toplu kilit/aç — satır sayısı kadar YENİ nesne üretir, mevcutları yerinde değiştirmez. */
function toggleAllLocks(): void {
  const nextLocked = !allRowsLocked.value
  rows.value = rows.value.map((row) => ({ ...row, isLocked: nextLocked }))
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
      // Kilitli satıra ASLA dokunulmaz. Arka uç zaten kilitliyi koruyor; bu,
      // oranın bir gün bozulması hâlinde ekranın kullanıcıyı yine de koruması
      // için ikinci kat. Bilinen yol `undoSuggestion` idi: Geri Al, satır
      // kilitlendikten SONRA bile dağıtım öncesi anlık görüntüyü olduğu gibi
      // geri yazıyor, kilidi de açıyordu — aşağıda düzeltildi.
      if (row.isLocked) continue
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

/**
 * Geri Al: satır ŞU AN kilitliyse hiçbir alanına dokunulmaz (kilitlenişten
 * sonra elle veya dağıtımla değişmiş olabilir, o hâli korunur). Kilitsiz
 * satırlarda yalnız `awardedHours` ve `isHonorary` anlık görüntüden geri
 * gelir; `isLocked` hiçbir satırda geri alınmaz. Anlık görüntüde olmayan
 * (sonradan eklenmiş) satır olduğu gibi kalır.
 */
function undoSuggestion(): void {
  const snapshotRows = snapshot.value
  if (!snapshotRows) return
  rows.value = rows.value.map((row) => {
    if (row.isLocked) return row
    const before = snapshotRows.find((r) => r.companyId === row.companyId)
    if (!before) return row
    return { ...row, awardedHours: before.awardedHours, isHonorary: before.isHonorary }
  })
  snapshot.value = null
  suggestionWarnings.value = []
}

/**
 * Dönem başladıysa (`isPlanning === false`) önce yürürlük tarihi ve gerekçe
 * sorulur; kullanıcı Vazgeç derse hiçbir şey kaydedilmez. Toplu satır kaydı
 * TEK bir pencereden geçer, satır başına ayrı pencere açılmaz.
 */
async function save(): Promise<void> {
  const details = await requestChangeDetails()
  if (details === null) return

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
    applyBoard(await hoursApi.save(payload, details))
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

/* Takdir sütunundaki InputNumber sarmalayıcısı — yığılı (varsayılan) düğme
   düzeniyle birlikte Kilit sütunuyla çakışmayacak sabit bir genişlik verir. */
.hours-input { width: 7rem; }
/* Mesafe hücresi tek satırda kalsın diye sarmıyor; uzun değerlerde satır
   yükseklikleri diğer satırlarla eşit kalır. */
.distance-cell { display: inline-flex; align-items: baseline; gap: 0.25rem; white-space: nowrap; }
.pool-warning-action { display: flex; }
.muted { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.footer-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 1rem;
  flex-wrap: wrap;
}
</style>
