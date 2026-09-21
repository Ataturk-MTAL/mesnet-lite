<template>
  <div class="page">
    <div class="page-header">
      <div class="title-group">
        <h1 class="page-title">{{ labels.allocation.title }}</h1>
        <Tag v-if="board?.term" :value="board.term" severity="secondary" icon="pi pi-calendar" />
      </div>
      <div class="header-actions">
        <Button
          :label="labels.allocation.propose"
          icon="pi pi-bolt"
          outlined
          :loading="isProposing"
          v-tooltip.top="labels.allocation.proposeTooltip"
          @click="openProposalDialog"
        />
        <Button
          :label="labels.allocation.clearAll"
          icon="pi pi-trash"
          severity="danger"
          outlined
          :disabled="(board?.assignedCompanyCount ?? 0) === 0"
          @click="confirmClear"
        />
      </div>
    </div>

    <Message severity="secondary" :closable="false">{{ labels.allocation.subtitle }}</Message>

    <Message
      v-for="(warning, index) in board?.warnings ?? []"
      :key="index"
      :severity="warning.includes('aşıyor') || warning.includes('aşıldı') ? 'error' : 'warn'"
      :closable="false"
    >
      {{ warning }}
    </Message>

    <!-- MESNET'teki dört sayaç -->
    <Card>
      <template #content>
        <div class="summary">
          <div class="summary-item">
            <div class="summary-value">{{ board?.poolHours ?? 0 }}</div>
            <div class="summary-label">{{ labels.allocation.poolHours }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value">{{ board?.assignedHours ?? 0 }}</div>
            <div class="summary-label">{{ labels.allocation.assignedHours }}</div>
          </div>
          <div class="summary-item">
            <div
              class="summary-value"
              :class="{ 'summary-value--over': (board?.remainingHours ?? 0) < 0 }"
            >
              {{ board?.remainingHours ?? 0 }}
            </div>
            <div class="summary-label">{{ labels.allocation.remainingHours }}</div>
          </div>
          <div class="summary-item">
            <div class="summary-value">
              {{ board?.assignedCompanyCount ?? 0 }} / {{ board?.totalCompanyCount ?? 0 }}
            </div>
            <div class="summary-label">
              {{ labels.allocation.assignedCount }}
              <span v-if="(board?.honoraryCount ?? 0) > 0">
                · {{ board?.honoraryCount }} {{ labels.allocation.honoraryNote }}
              </span>
            </div>
          </div>
        </div>
      </template>
    </Card>

    <div class="board">
      <!-- Sol: atanmamış işletme kartları. Yuva akıştan çıkarılıp mutlak konumlandırılıyor
           ki listenin kaç kart taşıdığı satırın (dolayısıyla sağdaki ızgaranın)
           yüksekliğini etkilemesin — satırı SAĞ panel belirlesin. -->
      <div class="panel-slot">
        <Card class="panel panel--list">
          <template #title>
            {{ labels.allocation.unassigned }} ({{ unassignedCompanies.length }})
          </template>
          <template #content>
            <InputText
              v-model="companySearch"
              :placeholder="labels.allocation.searchCompany"
              class="search"
            />

            <div class="company-list">
              <div v-if="unassignedGroups.length === 0" class="empty">
                {{ labels.allocation.allAssigned }}
              </div>

              <Panel
                v-for="group in unassignedGroups"
                :key="group.district"
                :header="group.label"
                toggleable
                class="district-group"
              >
                <div
                  v-for="company in group.companies"
                  :key="company.companyId"
                  class="company-card"
                  :class="{ 'company-card--dragging': draggedCompanyId === company.companyId }"
                  draggable="true"
                  tabindex="0"
                  role="button"
                  :aria-label="company.companyName"
                  @dragstart="onDragStart($event, company.companyId)"
                  @dragend="onDragEnd"
                  @keydown.enter.prevent="toggleKeyboardSelection(company.companyId)"
                  @keydown.space.prevent="toggleKeyboardSelection(company.companyId)"
                >
                  <div class="company-name">{{ company.companyName }}</div>
                  <div
                    v-if="company.addressText.trim().length > 0"
                    class="company-address"
                    v-tooltip.top="addressTooltip(company.addressText)"
                  >
                    {{ company.addressText }}
                  </div>
                  <div class="company-meta">
                    <Tag v-if="company.isHonorary" :value="labels.hours.honorary" severity="info" />
                    <Tag
                      v-else-if="company.hoursMissing"
                      :value="labels.allocation.hoursMissing"
                      severity="warn"
                    />
                    <Tag v-else :value="`${company.awardedHours} saat`" severity="success" />

                    <span class="muted">{{ company.studentCount }} öğrenci</span>
                    <span v-if="company.oneWayDistanceKm !== null" class="muted">
                      · {{ company.oneWayDistanceKm.toFixed(1) }} km
                    </span>
                  </div>
                  <div v-if="company.branches.length > 0" class="company-branches">
                    {{ company.branches.join(', ') }}
                  </div>
                  <div v-if="company.workplaceDays.length > 0" class="company-days">
                    {{ company.workplaceDays.map((d) => labels.allocation.days[d]).join(', ') }}
                  </div>
                  <div v-else class="company-days company-days--missing">
                    {{ labels.allocation.notWorkplaceDay }}
                  </div>
                </div>
              </Panel>

              <!-- Atanmış işletmeler: kart solda kaybolmasın, nereye gittiği görünsün -->
              <Panel
                v-if="assignedCompanies.length > 0"
                :header="`${labels.allocation.assignedSection} (${assignedCompanies.length})`"
                toggleable
                class="assigned-panel"
              >
                <div
                  v-for="company in assignedCompanies"
                  :key="company.companyId"
                  class="assigned-card"
                  :class="{ 'assigned-card--settle': recentlyAssignedCompanyId === company.companyId }"
                >
                  <div class="assigned-card-info">
                    <div class="company-name">{{ company.companyName }}</div>
                    <div
                      v-if="company.addressText.trim().length > 0"
                      class="company-address"
                      v-tooltip.top="addressTooltip(company.addressText)"
                    >
                      {{ company.addressText }}
                    </div>
                    <div class="company-meta">
                      <Tag :value="assignedTeacherName(company)" severity="secondary" />
                      <span class="muted">{{ assignedSlotLabel(company) }}</span>
                    </div>
                  </div>
                  <Button
                    icon="pi pi-times"
                    severity="danger"
                    text
                    rounded
                    size="small"
                    :aria-label="labels.allocation.removeAssignment"
                    v-tooltip.top="labels.allocation.removeAssignment"
                    @click="unassign(company.companyId)"
                  />
                </div>
              </Panel>
            </div>
          </template>
        </Card>
      </div>

      <!-- Sağ: öğretmen seçimi + haftalık ızgara -->
      <Card class="panel panel--grid">
        <template #title>{{ labels.allocation.weeklyGrid }}</template>
        <template #content>
          <Select
            v-model="selectedTeacherId"
            :options="board?.teachers ?? []"
            optionLabel="teacherName"
            optionValue="teacherId"
            :placeholder="labels.allocation.selectTeacher"
            class="teacher-select"
            filter
          />

          <div v-if="!selectedTeacher" class="empty">{{ labels.allocation.selectTeacher }}</div>

          <template v-else>
            <div class="teacher-stats">
              <Tag
                :value="`${labels.allocation.capacity}: ${selectedTeacher.assignedHours} / ${selectedTeacher.capacity}`"
                :severity="selectedTeacher.isOverCapacity ? 'danger' : 'secondary'"
              />
              <Tag
                :value="`${selectedTeacher.companyCount} ${labels.hours.company}`"
                severity="secondary"
              />
              <span v-if="selectedTeacher.branches.length > 0" class="muted">
                {{ selectedTeacher.branches.join(', ') }}
              </span>
            </div>

            <div class="grid-scroll">
              <table class="grid">
                <thead>
                  <tr>
                    <th class="grid-hour-head">{{ labels.allocation.hour }}</th>
                    <th v-for="day in DAYS" :key="day">{{ labels.allocation.days[day] }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="hour in gridHours" :key="hour">
                    <th class="grid-hour">{{ hour }}.</th>
                    <td
                      v-for="day in DAYS"
                      :key="`${day}-${hour}`"
                      class="grid-cell"
                      :class="cellClass(day, hour)"
                      @dragenter.prevent="onCellDragOver($event, day, hour)"
                      @dragover.prevent="onCellDragOver($event, day, hour)"
                      @drop.prevent.stop="onDrop($event, day, hour)"
                      @click="onCellClick(day, hour)"
                      @mouseenter="onCellMouseEnter(day, hour)"
                      @mouseleave="onCellMouseLeave(day, hour)"
                    >
                      <div
                        v-if="cellCompany(day, hour)"
                        class="cell-fill"
                        :class="{
                          'cell-fill--start': isBlockStart(day, hour),
                          'cell-fill--end': isBlockEnd(day, hour),
                        }"
                      >
                        <template v-if="isBlockStart(day, hour)">
                          <span class="cell-name">
                            {{ cellCompany(day, hour)?.companyName }} —
                            {{ blockHoursLabel(cellCompany(day, hour)!) }}
                          </span>
                          <Button
                            icon="pi pi-times"
                            severity="danger"
                            text
                            rounded
                            size="small"
                            :aria-label="labels.allocation.removeAssignment"
                            v-tooltip.top="labels.allocation.removeAssignment"
                            @click.stop="unassign(cellCompany(day, hour)!.companyId)"
                          />
                        </template>
                      </div>
                      <span v-else-if="isFree(day, hour)" class="cell-free">
                        {{ labels.allocation.free }}
                      </span>
                      <span v-else class="cell-unavailable">
                        {{ labels.allocation.unavailable }}
                      </span>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </template>
        </template>
      </Card>
    </div>

    <!-- Öğretmen özeti -->
    <Card>
      <template #title>{{ labels.allocation.teacherSummary }}</template>
      <template #content>
        <DataTable :value="board?.teachers ?? []" dataKey="teacherId" stripedRows>
          <Column field="teacherName" :header="labels.allocation.teacher" sortable />
          <Column field="companyCount" :header="labels.allocation.companyCount" sortable />
          <Column :header="labels.allocation.assignedHours" sortable field="assignedHours">
            <template #body="{ data }">
              <span :class="{ over: data.isOverCapacity }">
                {{ data.assignedHours }} / {{ data.capacity }}
              </span>
            </template>
          </Column>
          <Column v-for="day in DAYS" :key="day" :header="labels.allocation.days[day].slice(0, 3)">
            <template #body="{ data }">
              <span :class="{ over: data.daysOverCap.includes(day) }">
                {{ data.hoursPerDay[String(day)] ?? 0 }}
              </span>
            </template>
          </Column>
        </DataTable>
      </template>
    </Card>

    <!-- Zorlama diyaloğu -->
    <Dialog
      v-model:visible="isForceDialogOpen"
      modal
      :header="labels.allocation.forceTitle"
      :style="{ width: '30rem' }"
    >
      <p>{{ labels.allocation.forceQuestion }}</p>
      <ul class="force-reasons">
        <li v-for="(reason, index) in pendingViolations" :key="index">{{ reason }}</li>
      </ul>
      <div class="field">
        <label for="force-reason">{{ labels.allocation.forceReason }}</label>
        <Textarea id="force-reason" v-model="forceReason" rows="2" autofocus />
        <small class="muted">{{ labels.allocation.forceReasonHint }}</small>
      </div>
      <template #footer>
        <Button :label="labels.common.cancel" severity="secondary" outlined @click="cancelForce" />
        <Button
          :label="labels.allocation.forceConfirm"
          severity="warn"
          :disabled="forceReason.trim().length === 0"
          @click="confirmForce"
        />
      </template>
    </Dialog>

    <!-- Öneri diyaloğu -->
    <Dialog
      v-model:visible="isProposalDialogOpen"
      modal
      :header="labels.allocation.proposalTitle"
      :style="{ width: '36rem' }"
      @hide="closeProposalDialog"
    >
      <div v-if="!proposal || proposal.assignments.length === 0" class="empty">
        {{ labels.allocation.proposalEmpty }}
      </div>

      <template v-else>
        <div
          v-for="item in proposal.assignments"
          :key="item.companyId"
          class="proposal-row"
          :class="{
            'proposal-row--success': proposalResultFor(item.companyId)?.success === true,
            'proposal-row--failed': proposalResultFor(item.companyId)?.success === false,
          }"
        >
          <div class="proposal-row-info">
            <div class="company-name">{{ item.companyName }}</div>
            <div class="company-meta">
              <Tag :value="item.teacherName" severity="secondary" />
              <span class="muted">{{ proposalSlotLabel(item) }}</span>
              <Tag
                v-if="!item.exactBranchMatch"
                :value="labels.allocation.nearField"
                severity="warn"
              />
            </div>
            <div v-if="proposalResultFor(item.companyId)?.success === false" class="proposal-error">
              {{ proposalResultFor(item.companyId)?.errorMessage }}
            </div>
          </div>
          <i
            v-if="proposalResultFor(item.companyId)?.success === true"
            class="pi pi-check proposal-icon proposal-icon--success"
          />
          <i
            v-else-if="proposalResultFor(item.companyId)?.success === false"
            class="pi pi-times proposal-icon proposal-icon--failed"
          />
        </div>
      </template>

      <Panel
        v-if="proposal && proposal.unassigned.length > 0"
        :header="`${labels.allocation.proposalUnassigned} (${proposal.unassigned.length})`"
        toggleable
        class="proposal-unassigned-panel"
      >
        <div
          v-for="item in proposal.unassigned"
          :key="item.companyId"
          class="proposal-unassigned-row"
        >
          <span class="company-name">{{ item.companyName }}</span>
          <span class="muted">{{ item.reason }}</span>
        </div>
      </Panel>

      <template #footer>
        <Button
          :label="labels.common.cancel"
          severity="secondary"
          outlined
          @click="closeProposalDialog"
        />
        <Button
          :label="labels.allocation.proposalApply"
          :loading="isApplyingProposal"
          :disabled="!proposal || proposal.assignments.length === 0 || isApplyingProposal"
          @click="applyProposal"
        />
      </template>
    </Dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useToast } from 'openvue/usetoast'
import { useConfirm } from 'openvue/useconfirm'
import { assignmentsApi } from '../api/assignments'
import type {
  AllocationProposal,
  AssignmentBoard,
  BoardCompany,
  NewAssignment,
  ProposedAssignment,
} from '../api/assignments'
import { labels } from '../i18n/labels'
import { activeTerm } from '../composables/useTerm'

/** İşletme adresi ipucu. Varsayılan `--p-tooltip-max-width` (12.5rem) uzun bir
 *  adres için çok dar kalır; sınır `.p-tooltip` KÖKÜNDE tanımlı olduğundan
 *  genişlik yalnızca `text` üzerine verilirse kökün 12.5rem'i geçerli olmaya
 *  devam eder — bu yüzden `root` ve `text` ikisine birden veriliyor. Temel
 *  `white-space: pre-line` / `word-break: break-word` kuralı zaten sarma
 *  sağlıyor, üzerine yazmaya gerek yok. */
function addressTooltip(addressText: string): {
  value: string
  pt: { root: { style: { maxWidth: string } }; text: { style: { width: string; lineHeight: string } } }
} {
  return {
    value: addressText,
    pt: {
      root: { style: { maxWidth: '22rem' } },
      text: { style: { width: '22rem', lineHeight: '1.4' } },
    },
  }
}

const DAYS = [1, 2, 3, 4, 5] as const
/** OÖKY MADDE 88: bir öğretmen bir günde en fazla bu kadar saat koordinatörlük yapabilir. */
const DAILY_HOUR_LIMIT = 8
/** Yerleşim sonrası blok/kart vurgusunun ekranda kalma süresi (ms). CSS'teki
 *  `cell-settle` animasyon süresi ve `.assigned-card--settle` geçişiyle senkron tutulmalı. */
const SETTLE_ANIMATION_MS = 650

interface HoverCell {
  day: number
  hour: number
}

interface BlockHighlight {
  day: number
  hours: number[]
}

/** Öneri uygulanırken her bir kalemin sonucu; kaçının başarılı/başarısız olduğunu
 *  ve başarısızlık gerekçesini kullanıcıya tam olarak göstermek için tutulur. */
interface ProposalApplyResult {
  companyId: number
  companyName: string
  success: boolean
  errorMessage: string | null
}

const toast = useToast()
const confirm = useConfirm()

const board = ref<AssignmentBoard | null>(null)
const selectedTeacherId = ref<number | null>(null)
const companySearch = ref('')
const draggedCompanyId = ref<number | null>(null)
/** Sürükleme veya klavye seçimi sırasında imlecin üstünde olduğu hücre. */
const hoverCell = ref<HoverCell | null>(null)
/** Sürüklenen işletmenin ekran dışına konumlanmış özel sürükleme görüntüsü. */
const dragImageEl = ref<HTMLDivElement | null>(null)
/** Az önce yerleşen bloğu/kartı kısa süreliğine vurgulamak için. */
const settledBlock = ref<BlockHighlight | null>(null)
const recentlyAssignedCompanyId = ref<number | null>(null)
let settleTimeoutId: ReturnType<typeof setTimeout> | null = null

// Zorlama diyaloğu durumu
const isForceDialogOpen = ref(false)
const forceReason = ref('')
const pendingPlacement = ref<{ companyId: number; day: number; hour: number } | null>(null)
const pendingViolations = ref<string[]>([])

// Öneri diyaloğu durumu
const isProposalDialogOpen = ref(false)
const isProposing = ref(false)
const isApplyingProposal = ref(false)
const proposal = ref<AllocationProposal | null>(null)
const proposalApplyResults = ref<ProposalApplyResult[] | null>(null)

const selectedTeacher = computed(
  () => board.value?.teachers.find((t) => t.teacherId === selectedTeacherId.value) ?? null,
)

const draggedCompany = computed<BoardCompany | null>(
  () => board.value?.companies.find((c) => c.companyId === draggedCompanyId.value) ?? null,
)

/** Bloğun kaç hücre kapladığı. Fahri ziyaret tam 1 hücre. Yapısal tip kullanılır ki
 *  hem `BoardCompany` hem de `ProposedAssignment` için aynı hesap tekrarsız çalışsın. */
function companySpan(entry: { awardedHours: number }): number {
  return Math.max(1, entry.awardedHours)
}

/** `startHour`'dan başlayıp `span` hücre süren bloğun saatleri. */
function blockHours(startHour: number, span: number): number[] {
  return Array.from({ length: span }, (_, index) => startHour + index)
}

/** İmlecin üstündeki hücreden başlayacak bloğun önizlemesi ve geçerliliği. */
const previewBlock = computed<{ day: number; hours: number[]; isValid: boolean } | null>(() => {
  const cell = hoverCell.value
  const company = draggedCompany.value
  if (!cell || !company) return null

  const hours = blockHours(cell.hour, companySpan(company))
  const violations = collectViolations(company, cell.day, cell.hour)
  return { day: cell.day, hours, isValid: violations.length === 0 }
})

/** Izgara satırları ayarlardaki gün aralığından gelir. */
const gridHours = computed(() => {
  const start = board.value?.dayStartHour ?? 8
  const end = board.value?.dayEndHour ?? 17
  return Array.from({ length: Math.max(0, end - start) }, (_, index) => start + index)
})

const unassignedCompanies = computed(
  () => board.value?.companies.filter((c) => c.assignedTeacherId === null) ?? [],
)

const filteredUnassigned = computed(() => {
  const query = companySearch.value.trim().toLocaleLowerCase('tr')
  if (query.length === 0) return unassignedCompanies.value
  return unassignedCompanies.value.filter(
    (company) =>
      company.companyName.toLocaleLowerCase('tr').includes(query) ||
      company.addressText.toLocaleLowerCase('tr').includes(query),
  )
})

/** Atanmış işletmeler; solda kaybolmadan bir iz bırakması için ayrı listelenir. */
const assignedCompanies = computed(
  () => board.value?.companies.filter((c) => c.assignedTeacherId !== null) ?? [],
)

interface CompanyDistrictGroup {
  /** Boş dize: ilçesi ayrıştırılamamış işletmeler grubu. */
  district: string
  /** "Akdeniz (13)" gibi; sayaç arama filtresinden ETKİLENMEZ, o ilçedeki TÜM atanmamış
   *  işletmeleri sayar. Arama yalnızca hangi kartların gösterileceğini daraltır. */
  label: string
  companies: BoardCompany[]
}

/** Atanmamış işletmeleri ilçeye göre gruplar; Türkçe alfabetik sıralanır, ilçesi boş
 *  olanlar sonda ayrı bir grupta toplanır. Aramayla eşleşen kartı kalmayan grup hiç
 *  gösterilmez. */
const unassignedGroups = computed<CompanyDistrictGroup[]>(() => {
  const namedDistricts = [
    ...new Set(
      unassignedCompanies.value
        .map((c) => c.district)
        .filter((district) => district.trim().length > 0),
    ),
  ].sort((a, b) => a.localeCompare(b, 'tr'))

  const orderedDistricts = [...namedDistricts, '']

  return orderedDistricts
    .map((district) => {
      const totalCount = unassignedCompanies.value.filter((c) => c.district === district).length
      const visibleCompanies = filteredUnassigned.value.filter((c) => c.district === district)
      const districtName = district.trim().length > 0 ? district : labels.allocation.districtUnknown
      return {
        district,
        label: `${districtName} (${totalCount})`,
        companies: visibleCompanies,
      }
    })
    .filter((group) => group.companies.length > 0)
})

function showError(error: unknown): void {
  const detail = error instanceof Error ? error.message : labels.common.error
  toast.add({ severity: 'error', summary: labels.common.error, detail, life: 8000 })
}

/** "İşletme adı — 6 saat" ya da fahri ziyarette "İşletme adı — Fahri" biçimi. */
function blockHoursLabel(company: BoardCompany): string {
  return company.isHonorary
    ? labels.hours.honorary
    : `${company.awardedHours} ${labels.allocation.hoursSuffix}`
}

function assignedTeacherName(company: BoardCompany): string {
  return (
    board.value?.teachers.find((t) => t.teacherId === company.assignedTeacherId)?.teacherName ?? ''
  )
}

/** "Salı 10–12" gibi kısa bir yerleşim özeti; tek saatlik blokta aralık gösterilmez. */
function assignedSlotLabel(company: BoardCompany): string {
  if (company.visitDay === null || company.visitHour === null) return ''
  const dayName = labels.allocation.days[company.visitDay]
  const endHour = company.visitEndHour ?? company.visitHour
  return endHour > company.visitHour
    ? `${dayName} ${company.visitHour}–${endHour}`
    : `${dayName} ${company.visitHour}`
}

function isFree(day: number, hour: number): boolean {
  return selectedTeacher.value?.freeSlots.includes(`${day}-${hour}`) ?? false
}

/** Hücreyi kaplayan işletme; blokun başlangıcı değil, HER hücresi için çalışır. */
function cellCompany(day: number, hour: number): BoardCompany | null {
  const companyId = selectedTeacher.value?.occupiedBy[`${day}-${hour}`]
  if (companyId === undefined) return null
  return board.value?.companies.find((c) => c.companyId === companyId) ?? null
}

function isBlockStart(day: number, hour: number): boolean {
  const company = cellCompany(day, hour)
  return company !== null && company.visitHour === hour
}

function isBlockEnd(day: number, hour: number): boolean {
  const company = cellCompany(day, hour)
  return company !== null && company.visitEndHour === hour
}

/** İmleç bu hücredeyken önizlenen blok bu hücreyi de kaplıyor mu. */
function isInPreview(day: number, hour: number): boolean {
  const preview = previewBlock.value
  return preview !== null && preview.day === day && preview.hours.includes(hour)
}

function cellClass(day: number, hour: number): Record<string, boolean> {
  const occupied = cellCompany(day, hour) !== null
  const inPreview = !occupied && isInPreview(day, hour)
  const previewValid = inPreview && (previewBlock.value?.isValid ?? false)
  return {
    'grid-cell--busy': occupied,
    'grid-cell--block-start': occupied && isBlockStart(day, hour),
    'grid-cell--block-end': occupied && isBlockEnd(day, hour),
    'grid-cell--free': !occupied && !inPreview && isFree(day, hour),
    'grid-cell--blocked': !occupied && !inPreview && !isFree(day, hour),
    'grid-cell--preview-valid': previewValid,
    'grid-cell--preview-invalid': inPreview && !previewValid,
    'grid-cell--settle': isSettled(day, hour),
  }
}

function isSettled(day: number, hour: number): boolean {
  const settle = settledBlock.value
  return settle !== null && settle.day === day && settle.hours.includes(hour)
}

/** Ekran dışına konumlanmış, sürükleme sırasında gösterilen kompakt görüntü. */
function buildDragImage(company: BoardCompany): HTMLDivElement {
  const el = document.createElement('div')
  el.textContent = `${company.companyName} — ${blockHoursLabel(company)}`
  Object.assign(el.style, {
    // WebKit (Tauri'nin macOS webview'i), görüş alanının TAMAMEN dışına (ör. top: -1000px)
    // konumlanmış elemanlar için sürükleme görüntüsünü BOŞ üretebiliyor. Yatayda dışarı
    // taşıyıp dikeyde görüş alanında (top: 0) tutmak WebKit'te de güvenilir çalışıyor.
    // `display: none` / `visibility: hidden` KULLANILMAZ; o durumda görüntü hiç oluşmaz.
    position: 'fixed',
    top: '0',
    left: '-9999px',
    padding: '0.375rem 0.625rem',
    borderRadius: '0.375rem',
    background: 'var(--p-primary-color)',
    color: 'var(--p-primary-contrast-color)',
    fontSize: '0.8125rem',
    fontWeight: '600',
    whiteSpace: 'nowrap',
    boxShadow: '0 2px 8px rgba(0, 0, 0, 0.25)',
  })
  return el
}

/**
 * WebKit (Tauri'nin macOS webview'i) `dataTransfer` boş bırakılırsa sürükleme
 * işlemini hiç başlatmaz. Chromium buna göz yumduğu için hata yalnızca
 * paketlenmiş uygulamada görünür. İşletme kimliğini yüke yazıyoruz.
 */
function onDragStart(event: DragEvent, companyId: number): void {
  draggedCompanyId.value = companyId
  if (!event.dataTransfer) return

  event.dataTransfer.setData('text/plain', String(companyId))
  event.dataTransfer.effectAllowed = 'move'

  const company = board.value?.companies.find((c) => c.companyId === companyId)
  if (!company) return
  const dragImage = buildDragImage(company)
  document.body.appendChild(dragImage)
  event.dataTransfer.setDragImage(dragImage, 12, 12)
  dragImageEl.value = dragImage
}

function onDragEnd(): void {
  draggedCompanyId.value = null
  hoverCell.value = null
  dragImageEl.value?.remove()
  dragImageEl.value = null
}

/** `hoverCell`'i yalnızca gün VEYA saat gerçekten değiştiğinde yazar. `dragover` bir
 *  hücre üstündeyken saniyede onlarca kez tetiklenir; her seferinde YENİ bir nesne
 *  yazmak ref'i "değişti" saydırıp önizleme/ihlal taramasını ve tüm ızgarayı gereksiz
 *  yere yeniden hesaplatıyordu. Aynı hücre için tekrar yazma yapılmaz. */
function setHoverCell(day: number, hour: number): void {
  const current = hoverCell.value
  if (current !== null && current.day === day && current.hour === hour) return
  hoverCell.value = { day, hour }
}

/** Hedef hücrede taşıma imlecini gösterir ve blok önizlemesini günceller. */
function onCellDragOver(event: DragEvent, day: number, hour: number): void {
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = 'move'
  }
  setHoverCell(day, hour)
}

/** Klavye ile seçim sırasında fare hücrenin üstüne gelince önizleme göster. */
function onCellMouseEnter(day: number, hour: number): void {
  if (draggedCompanyId.value !== null) {
    setHoverCell(day, hour)
  }
}

function onCellMouseLeave(day: number, hour: number): void {
  if (hoverCell.value?.day === day && hoverCell.value?.hour === hour) {
    hoverCell.value = null
  }
}

/** Klavye ile seçim: Enter kartı seçer, sonra hücrede tıklama bırakır. */
function toggleKeyboardSelection(companyId: number): void {
  const isDeselecting = draggedCompanyId.value === companyId
  draggedCompanyId.value = isDeselecting ? null : companyId
  if (isDeselecting) hoverCell.value = null
}

function onCellClick(day: number, hour: number): void {
  if (draggedCompanyId.value === null) return
  void place(draggedCompanyId.value, day, hour)
}

function onDrop(event: DragEvent, day: number, hour: number): void {
  // `dragend` bazı webview'larda `drop`tan önce tetiklenip ref'i temizler;
  // asıl kaynak sürükleme yüküdür, ref yalnızca yedek.
  const payload = Number(event.dataTransfer?.getData('text/plain'))
  const companyId = Number.isFinite(payload) && payload > 0 ? payload : draggedCompanyId.value
  hoverCell.value = null
  if (companyId === null) return
  void place(companyId, day, hour)
}

/** Kural ihlallerini toplar. Boş dizi dönerse yerleşim (bloğun TAMAMI için) temizdir. */
function collectViolations(company: BoardCompany, day: number, hour: number): string[] {
  const problems: string[] = []
  const span = companySpan(company)
  const hours = blockHours(hour, span)
  const dayEndHour = board.value?.dayEndHour ?? 17
  const teacher = selectedTeacher.value

  const allHoursFree = hours.every((h) => isFree(day, h))
  if (!allHoursFree) {
    problems.push('Seçilen bloktaki saatlerin tamamı öğretmenin boş saatleri arasında değil.')
  }
  if (!company.workplaceDays.includes(day)) {
    problems.push(`Öğrenciler ${labels.allocation.days[day]} günü işletmede değil.`)
  }
  if (hour + span > dayEndHour) {
    problems.push(
      `Blok ${span} saat sürüyor ve ${labels.allocation.days[day]} günü ${dayEndHour}.saati aşıyor.`,
    )
  }

  if (teacher) {
    const overlapCompanyId = hours
      .map((h) => teacher.occupiedBy[`${day}-${h}`])
      .find((id) => id !== undefined)
    if (overlapCompanyId !== undefined) {
      const overlapCompany = board.value?.companies.find((c) => c.companyId === overlapCompanyId)
      problems.push(
        `Blok, ${overlapCompany?.companyName ?? 'başka bir atama'} işletmesinin bloğuyla çakışıyor.`,
      )
    }

    const dayHours = teacher.hoursPerDay[String(day)] ?? 0
    if (dayHours + company.awardedHours > DAILY_HOUR_LIMIT) {
      problems.push(
        `${labels.allocation.days[day]} günü toplam ${dayHours + company.awardedHours} saat olur; günlük sınır ${DAILY_HOUR_LIMIT} (OÖKY MADDE 88).`,
      )
    }
    if (teacher.assignedHours + company.awardedHours > teacher.capacity) {
      problems.push(
        `Öğretmenin toplamı ${teacher.assignedHours + company.awardedHours} saate çıkar; kapasitesi ${teacher.capacity}.`,
      )
    }
  }

  return problems
}

/** Yeni yerleşen bloğu ızgarada ve atanmış listesinde kısaca vurgular. */
function flashAssignment(companyId: number): void {
  const company = board.value?.companies.find((c) => c.companyId === companyId)
  if (!company || company.visitDay === null || company.visitHour === null) return

  const span =
    company.visitEndHour !== null
      ? company.visitEndHour - company.visitHour + 1
      : companySpan(company)
  settledBlock.value = { day: company.visitDay, hours: blockHours(company.visitHour, span) }
  recentlyAssignedCompanyId.value = companyId

  if (settleTimeoutId !== null) clearTimeout(settleTimeoutId)
  settleTimeoutId = setTimeout(() => {
    settledBlock.value = null
    recentlyAssignedCompanyId.value = null
    settleTimeoutId = null
  }, SETTLE_ANIMATION_MS)
}

async function place(companyId: number, day: number, hour: number): Promise<void> {
  const teacherId = selectedTeacherId.value
  if (teacherId === null) {
    toast.add({ severity: 'warn', summary: labels.allocation.selectTeacher, life: 4000 })
    return
  }

  const company = board.value?.companies.find((c) => c.companyId === companyId)
  if (!company) return

  const problems = collectViolations(company, day, hour)
  if (problems.length > 0) {
    // Kural dışı yerleşim engellenmez, gerekçe istenir ve kayda geçer.
    pendingPlacement.value = { companyId, day, hour }
    pendingViolations.value = problems
    forceReason.value = ''
    isForceDialogOpen.value = true
    return
  }

  await submit({
    teacherId,
    companyId,
    visitDay: day,
    visitHour: hour,
    isForced: false,
    forceReason: null,
  })
}

async function submit(input: NewAssignment): Promise<void> {
  try {
    board.value = await assignmentsApi.assign(input)
    draggedCompanyId.value = null
    hoverCell.value = null
    toast.add({ severity: 'success', summary: labels.allocation.assigned, life: 2500 })
    flashAssignment(input.companyId)
  } catch (error: unknown) {
    showError(error)
  }
}

function cancelForce(): void {
  isForceDialogOpen.value = false
  pendingPlacement.value = null
  pendingViolations.value = []
}

async function confirmForce(): Promise<void> {
  const placement = pendingPlacement.value
  const teacherId = selectedTeacherId.value
  if (!placement || teacherId === null) return

  isForceDialogOpen.value = false
  await submit({
    teacherId,
    companyId: placement.companyId,
    visitDay: placement.day,
    visitHour: placement.hour,
    isForced: true,
    forceReason: forceReason.value.trim(),
  })
  pendingPlacement.value = null
  pendingViolations.value = []
}

async function unassign(companyId: number): Promise<void> {
  try {
    board.value = await assignmentsApi.unassign(companyId)
    toast.add({ severity: 'success', summary: labels.allocation.unassigned2, life: 2500 })
  } catch (error: unknown) {
    showError(error)
  }
}

function confirmClear(): void {
  confirm.require({
    message: labels.allocation.clearConfirm,
    header: labels.allocation.clearAll,
    acceptLabel: labels.common.yes,
    rejectLabel: labels.common.no,
    acceptProps: { severity: 'danger' },
    accept: async () => {
      try {
        board.value = await assignmentsApi.clear()
        toast.add({ severity: 'success', summary: labels.allocation.cleared, life: 2500 })
      } catch (error: unknown) {
        showError(error)
      }
    },
  })
}

/** Önerilen atamanın gün/saat aralığı etiketi. `ProposedAssignment` yalnızca bloğun
 *  BAŞLANGICINI taşıdığından bitiş saati burada `awardedHours`'tan hesaplanır. */
function proposalSlotLabel(item: ProposedAssignment): string {
  const span = companySpan(item)
  const endHour = item.visitHour + span - 1
  const dayName = labels.allocation.days[item.visitDay]
  return endHour > item.visitHour
    ? `${dayName} ${item.visitHour}–${endHour}`
    : `${dayName} ${item.visitHour}`
}

/** İlgili önerinin uygulama sonucu; henüz uygulanmadıysa `null`. */
function proposalResultFor(companyId: number): ProposalApplyResult | null {
  return proposalApplyResults.value?.find((r) => r.companyId === companyId) ?? null
}

/** Öneriyi arka uçtan ister; hiçbir şey kaydetmez, yalnızca diyaloğu doldurur. */
async function openProposalDialog(): Promise<void> {
  isProposing.value = true
  try {
    proposal.value = await assignmentsApi.propose()
    proposalApplyResults.value = null
    isProposalDialogOpen.value = true
  } catch (error: unknown) {
    showError(error)
  } finally {
    isProposing.value = false
  }
}

function closeProposalDialog(): void {
  isProposalDialogOpen.value = false
  proposal.value = null
  proposalApplyResults.value = null
}

/**
 * Öneriyi sırayla uygular. Bir kalem hata verirse diğerleri de denenmeye devam eder
 * (hata sessizce yutulmaz, kalan kalemler de iptal edilmez); bittiğinde kaçının
 * başarılı/başarısız olduğu ve gerekçesi hem diyalogda satır satır hem de tek bir
 * toast özetinde kullanıcıya bildirilir. Sonunda pano tazelenir.
 */
async function applyProposal(): Promise<void> {
  const items = proposal.value?.assignments ?? []
  if (items.length === 0) return

  isApplyingProposal.value = true
  const results: ProposalApplyResult[] = []

  for (const item of items) {
    try {
      await assignmentsApi.assign({
        teacherId: item.teacherId,
        companyId: item.companyId,
        visitDay: item.visitDay,
        visitHour: item.visitHour,
        isForced: false,
        forceReason: null,
      })
      results.push({
        companyId: item.companyId,
        companyName: item.companyName,
        success: true,
        errorMessage: null,
      })
    } catch (error: unknown) {
      results.push({
        companyId: item.companyId,
        companyName: item.companyName,
        success: false,
        errorMessage: error instanceof Error ? error.message : labels.common.error,
      })
    }
  }

  proposalApplyResults.value = results
  await load()

  const successCount = results.filter((r) => r.success).length
  const failureCount = results.length - successCount
  if (failureCount === 0) {
    toast.add({ severity: 'success', summary: labels.allocation.proposalApplied, life: 3000 })
  } else {
    const failedNames = results
      .filter((r) => !r.success)
      .map((r) => r.companyName)
      .join(', ')
    toast.add({
      severity: 'warn',
      summary: labels.allocation.proposalApplied,
      detail: `${successCount}/${results.length} ${labels.allocation.proposalSuccessSuffix}. ${labels.allocation.proposalFailedPrefix}: ${failedNames}`,
      life: 10000,
    })
  }

  isApplyingProposal.value = false
}

async function load(): Promise<void> {
  try {
    board.value = await assignmentsApi.get()
    // İlk öğretmen otomatik seçilsin ki ızgara boş görünmesin.
    if (selectedTeacherId.value === null && board.value.teachers.length > 0) {
      selectedTeacherId.value = board.value.teachers[0].teacherId
    }
  } catch (error: unknown) {
    showError(error)
  }
}

watch(activeTerm, load)

onMounted(load)

onUnmounted(() => {
  if (settleTimeoutId !== null) clearTimeout(settleTimeoutId)
  dragImageEl.value?.remove()
})
</script>

<style scoped>
.page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
.page-header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.title-group { display: flex; align-items: center; gap: 0.75rem; }
.header-actions { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
.page-title { font-size: 1.5rem; font-weight: 600; margin: 0; }

.summary { display: flex; flex-wrap: wrap; gap: 2.5rem; }
.summary-item { min-width: 9rem; }
.summary-value { font-size: 1.5rem; font-weight: 700; line-height: 1.1; }
.summary-value--over { color: var(--p-red-500); }
.summary-label { font-size: 0.8125rem; color: var(--p-text-muted-color); margin-top: 0.125rem; }

/* Satırın yüksekliğini SAĞ panel belirler. `.panel-slot`un tek çocuğu mutlak
 * konumlu olduğu için yuvanın kendi içerik yüksekliği sıfırdır ve satıra
 * katkı vermez; `align-items: stretch` ile yuva satır boyuna uzar, kart da
 * yuvayı doldurur. Sol listenin kaç işletme taşıdığı artık sayfa boyunu
 * etkilemez. `min-height`, öğretmen seçili değilken sağ panel çok kısa
 * kaldığında listenin birkaç satıra düşmesini engeller. */
.board { display: flex; gap: 1rem; align-items: stretch; flex-wrap: wrap; }

.panel-slot { flex: 1; min-width: 20rem; min-height: 28rem; position: relative; }
.panel--list { position: absolute; inset: 0; display: flex; flex-direction: column; }

.panel--grid { flex: 2; min-width: 28rem; }
.panel--list :deep(.p-card-body) { min-height: 0; flex: 1; display: flex; flex-direction: column; }
.panel--list :deep(.p-card-content) { min-height: 0; flex: 1; display: flex; flex-direction: column; }

.company-list { flex: 1; min-height: 0; overflow-y: auto; }

.search { width: 100%; margin-bottom: 0.75rem; }
.empty { color: var(--p-text-muted-color); padding: 1rem 0; }
.district-group { margin-bottom: 0.75rem; }
.district-group :deep(.company-card:last-child) { margin-bottom: 0; }

/* OpenVue'nun Panel bileşeni (`.p-panel-content-container`) daraltma animasyonu
 * için CSS Grid kullanıyor; ızgara ögesi `.p-panel-content-wrapper` varsayılan
 * `min-width: auto` taşıyor. `white-space: nowrap` uygulanan uzun adres metni
 * içerik-minimumunu genişletince bu ızgara ögesi kartla birlikte panelin dışına
 * taşıyor. Sıfırlamak, taşan öge zincirini keser; `fluid` benzeri bir çözüm yok
 * çünkü kaynak grid, bizim şablonumuzun değil OpenVue'nun kendi iç yapısı. */
.district-group :deep(.p-panel-content-wrapper),
.assigned-panel :deep(.p-panel-content-wrapper) {
  min-width: 0;
}

.company-card {
  border: 1px solid var(--p-content-border-color);
  border-radius: var(--p-content-border-radius);
  padding: 0.625rem 0.75rem;
  margin-bottom: 0.5rem;
  cursor: grab;
  background: var(--p-content-background);
  /* WebKit metin seçimini sürükleme sanır; elemanın kendisi sürüklenmeli. */
  -webkit-user-drag: element;
  user-select: none;
  transition:
    transform 150ms ease,
    opacity 150ms ease,
    box-shadow 150ms ease,
    background-color 150ms ease;
}
.company-card:active { cursor: grabbing; }
.company-card:hover { background: var(--p-content-hover-background); }
.company-card--dragging {
  outline: 2px solid var(--p-primary-color);
  opacity: 0.5;
  transform: scale(0.97);
}
.company-name { font-weight: 600; font-size: 0.9375rem; }
.company-address {
  font-size: 0.75rem; color: var(--p-text-muted-color); margin-top: 0.125rem;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.company-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
  margin-top: 0.375rem;
}
.company-branches,
.company-days { font-size: 0.75rem; color: var(--p-text-muted-color); margin-top: 0.25rem; }
.company-days--missing { color: var(--p-orange-500); }

/* Atanmış işletmeler paneli: kart solda kaybolmasın, iz bıraksın */
.assigned-panel { margin-top: 1rem; }
.assigned-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  border: 1px solid var(--p-content-border-color);
  border-radius: var(--p-content-border-radius);
  padding: 0.5rem 0.75rem;
  margin-bottom: 0.5rem;
  background: var(--p-content-background);
  transition:
    background-color 400ms ease,
    box-shadow 400ms ease;
}
.assigned-card--settle {
  background: var(--p-highlight-background);
  box-shadow: 0 0 0 2px var(--p-primary-color);
}
.assigned-card-info { min-width: 0; }

.teacher-select { width: 100%; margin-bottom: 0.75rem; }
.teacher-stats {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
  margin-bottom: 0.75rem;
}

.grid-scroll { overflow-x: auto; }
.grid { width: 100%; border-collapse: collapse; }
.grid th,
.grid td {
  border: 1px solid var(--p-content-border-color);
  padding: 0.375rem 0.5rem;
  font-size: 0.8125rem;
  text-align: center;
}
.grid-hour-head,
.grid-hour { width: 3.5rem; color: var(--p-text-muted-color); font-weight: 500; }
.grid-cell {
  position: relative;
  height: 3rem;
  min-width: 8rem;
  transition:
    background-color 150ms ease,
    outline-color 150ms ease,
    box-shadow 150ms ease;
}
.grid-cell--free { cursor: pointer; }
.grid-cell--blocked {
  background: var(--p-content-hover-background);
  color: var(--p-text-muted-color);
}
/* Blok hücreleri artık `cell-fill` katmanıyla boyanır; td'nin kendi dolgusu sıfırlanır. */
.grid-cell--busy { padding: 0; }
/* Aynı bloğun ardışık hücreleri arasındaki çizgiyi görünmez kılar, bitişik görünsün diye.
   `border-collapse: collapse` altında hangi hücrenin kenarlığı kazanırsa kazansın aynı
   renk görünsün diye HEM üst HEM alt kenarlık boyanır. */
.grid-cell--busy:not(.grid-cell--block-start) { border-top-color: var(--p-highlight-background); }
.grid-cell--busy:not(.grid-cell--block-end) { border-bottom-color: var(--p-highlight-background); }

.grid-cell--preview-valid {
  background: color-mix(in srgb, var(--p-green-500) 22%, transparent);
  outline: 2px dashed var(--p-green-500);
  outline-offset: -2px;
}
.grid-cell--preview-invalid {
  background: color-mix(in srgb, var(--p-red-500) 22%, transparent);
  outline: 2px dashed var(--p-red-500);
  outline-offset: -2px;
}

.grid-cell--settle { animation: cell-settle var(--settle-duration, 650ms) ease; }
@keyframes cell-settle {
  0% { box-shadow: inset 0 0 0 3px var(--p-primary-color); }
  100% { box-shadow: inset 0 0 0 0 transparent; }
}

.cell-fill {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.25rem;
  padding: 0 0.5rem;
  background: var(--p-highlight-background);
  color: var(--p-highlight-color);
  font-weight: 500;
  overflow: hidden;
}
.cell-fill--start { border-top-left-radius: 0.5rem; border-top-right-radius: 0.5rem; }
.cell-fill--end { border-bottom-left-radius: 0.5rem; border-bottom-right-radius: 0.5rem; }

.cell-free { color: var(--p-text-muted-color); }
.cell-unavailable { color: var(--p-text-muted-color); opacity: 0.6; }
.cell-name {
  margin-right: 0.25rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.over { color: var(--p-red-500); font-weight: 600; }
.muted { color: var(--p-text-muted-color); font-size: 0.8125rem; }
.force-reasons { margin: 0.5rem 0 1rem; padding-left: 1.25rem; color: var(--p-orange-500); }
.field { display: flex; flex-direction: column; gap: 0.375rem; }

/* Öneri diyaloğu satırları */
.proposal-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  border: 1px solid var(--p-content-border-color);
  border-radius: var(--p-content-border-radius);
  padding: 0.5rem 0.75rem;
  margin-bottom: 0.5rem;
  background: var(--p-content-background);
}
.proposal-row--success { border-color: var(--p-green-500); }
.proposal-row--failed { border-color: var(--p-red-500); }
.proposal-row-info { min-width: 0; }
.proposal-error { font-size: 0.75rem; color: var(--p-red-500); margin-top: 0.25rem; }
.proposal-icon { font-size: 1.125rem; flex-shrink: 0; }
.proposal-icon--success { color: var(--p-green-500); }
.proposal-icon--failed { color: var(--p-red-500); }
.proposal-unassigned-panel { margin-top: 1rem; }
.proposal-unassigned-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.375rem 0;
  border-bottom: 1px solid var(--p-content-border-color);
}
.proposal-unassigned-row:last-child { border-bottom: none; }

/* Hareket duyarlılığı azaltılmış kullanıcılar için tüm geçiş/animasyonları kapat. */
@media (prefers-reduced-motion: reduce) {
  .company-card,
  .assigned-card,
  .grid-cell,
  .grid-cell--settle {
    transition: none;
    animation: none;
  }
}
</style>
