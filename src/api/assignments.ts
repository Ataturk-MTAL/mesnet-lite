import { call } from './client'
import type { EffectiveChangeInput } from '../types/models'

/** Atama ekranındaki işletme kartı. */
export interface BoardCompany {
  companyId: number
  companyName: string
  addressText: string
  /** Adresten ayrıştırılan ilçe; kartta gösterilir. Gruplama `groupKey` ile yapılır. Ayrıştırılamayan adreste boş dize. */
  district: string
  neighborhood: string
  /** `cluster:{n}` / `district:{ilçe}` / `manual:{dizin}`; `null` = grup yok. */
  groupKey: string | null
  /** Grup yoksa boş dize. */
  groupLabel: string
  oneWayDistanceKm: number | null
  studentCount: number
  studentNames: string[]
  branches: string[]
  /** Takdir edilen haftalık saat. Fahri ziyarette 0. */
  awardedHours: number
  isHonorary: boolean
  /** Takdir hiç girilmemişse true. */
  hoursMissing: boolean
  /** Öğrencilerin sınıflarının işletmede bulunduğu günler (1–5). */
  workplaceDays: number[]
  assignedTeacherId: number | null
  visitDay: number | null
  visitHour: number | null
  /** Bloğun bittiği saat, uç DAHİL. Atanmamışsa `null`. */
  visitEndHour: number | null
  isForced: boolean
  forceReason: string | null
  /** Atamanın kaynağı; atanmamışsa `null`. Eski kayıtlar `manual` sayılır. */
  assignmentSource: 'manual' | 'proposal' | null
}

export interface BoardTeacher {
  teacherId: number
  teacherName: string
  branches: string[]
  capacity: number
  assignedHours: number
  companyCount: number
  /** Boş saatler: `{gün}-{saat}` anahtarları. */
  freeSlots: string[]
  /** Bloğun HER hücresini kaplayan işletme: `{gün}-{saat}` → companyId. */
  occupiedBy: Record<string, number>
  /** Gün numarası → o güne düşen toplam saat. */
  hoursPerDay: Record<string, number>
  daysOverCap: number[]
  isOverCapacity: boolean
}

export interface AssignmentBoard {
  term: string
  teachers: BoardTeacher[]
  companies: BoardCompany[]
  dayStartHour: number
  dayEndHour: number
  poolHours: number
  assignedHours: number
  remainingHours: number
  assignedCompanyCount: number
  totalCompanyCount: number
  honoraryCount: number
  warnings: string[]
}

export interface NewAssignment {
  teacherId: number
  companyId: number
  /** 1 = Pazartesi … 5 = Cuma */
  visitDay: number
  visitHour: number
  isForced: boolean
  forceReason: string | null
}

/** Öneri kipi. `redistribute` yalnız öneriden gelmiş, kilitsiz, zorlamasız atamaları yeniden düzenler. */
export type ProposalMode = 'fillGaps' | 'redistribute'

/** Bir işletmenin mevcut yerleşimi. `source`: atamanın elle mi, öneriden mi geldiği. */
export interface CurrentPlacement {
  teacherId: number
  visitDay: number
  visitHour: number
  isForced: boolean
  source: 'manual' | 'proposal'
}

/** Yeni ya da yer değiştiren atama. */
export interface ProposedAssignment {
  companyId: number
  companyName: string
  teacherId: number
  teacherName: string
  awardedHours: number
  previousHours: number
  visitDay: number
  visitHour: number
  /** Bloğun bittiği saat, uç DAHİL. */
  visitEndHour: number
  exactBranchMatch: boolean
  groupKey: string | null
  /** Taşınan işletmede önceki yer; yeni atamada `null`. */
  previous: CurrentPlacement | null
}

/** Yerinde kalan atama; saati değişmiş olabilir (o zaman `hourChanges`'te de vardır). */
export interface KeptAssignment {
  companyId: number
  companyName: string
  teacherId: number
  visitDay: number
  visitHour: number
  awardedHours: number
  isLocked: boolean
  isForced: boolean
}

export interface ReleasedAssignment {
  companyId: number
  companyName: string
  previous: CurrentPlacement
}

export type HourChangeReasonCode =
  | {
      kind:
        | 'poolExhausted'
        | 'poolAlreadyOverrun'
        | 'teacherCapacity'
        | 'dailyCap'
        | 'noConsecutiveCells'
        | 'ceilingLowered'
        | 'granted'
        | 'searchLimit'
        | 'unplaced'
    }
  | { kind: 'madeRoomFor'; companyId: number }

export interface HourChange {
  companyId: number
  companyName: string
  oldHours: number
  newHours: number
  reasonCode: HourChangeReasonCode
  /** Türkçe hazır gerekçe. */
  reason: string
}

export interface ProposalGroup {
  groupKey: string | null
  groupLabel: string
  companyNames: string[]
}

export interface GroupSplit {
  teacherId: number
  teacherName: string
  groups: ProposalGroup[]
}

export type UnassignedReasonCode =
  | 'noTeachers'
  | 'noWorkplaceDays'
  | 'noEligibleCell'
  | 'allEligibleCellsOccupied'
  | 'noTeacherCapacity'
  | 'noConsecutiveBlock'
  | 'dailyCapReached'
  | 'lockedHoursTooLong'

export interface UnassignedCompany {
  companyId: number
  companyName: string
  reasonCode: UnassignedReasonCode
  /** Türkçe gerekçe. */
  reason: string
  wasAssigned: boolean
}

export interface TeacherLoadSummary {
  teacherId: number
  teacherName: string
  /** Taban yük dahil. */
  hours: number
  capacity: number
  companyCount: number
  distinctGroups: number
}

export interface AllocationProposal {
  mode: ProposalMode
  assignments: ProposedAssignment[]
  kept: KeptAssignment[]
  released: ReleasedAssignment[]
  hourChanges: HourChange[]
  groupSplits: GroupSplit[]
  unassigned: UnassignedCompany[]
  teacherLoads: TeacherLoadSummary[]
  placedCount: number
  totalHours: number
  poolHours: number
  /** `null` = havuz tanımsız; negatif olabilir. */
  poolRemaining: number | null
  warnings: string[]
}

export const assignmentsApi = {
  /** `asOf` `null` ise güncel kayıt (düzenlenebilir); bir tarihse o güne göre okuma (salt okunur). */
  get: (asOf: string | null = null): Promise<AssignmentBoard> =>
    call('get_assignment_board', { asOf }),
  /** Öneri üretir; hiçbir şey kaydetmez. Dönem başladıysa `redistribute` Türkçe hata ile reddedilir. */
  propose: (mode: ProposalMode): Promise<AllocationProposal> => call('propose_assignments', { mode }),
  /**
   * Dönem başladıysa (`isPlanning === false`) `change` zorunlu etkiyle
   * gönderilmelidir; arka uç tarihsiz yazımı reddeder. Planlamada `change`
   * verilmezse ikisi de `null` gider.
   */
  assign: (input: NewAssignment, change?: EffectiveChangeInput): Promise<AssignmentBoard> =>
    call('assign_company', {
      input,
      effectiveDate: change?.effectiveDate ?? null,
      reason: change?.reason ?? null,
    }),
  unassign: (companyId: number, change?: EffectiveChangeInput): Promise<AssignmentBoard> =>
    call('unassign_company', {
      companyId,
      effectiveDate: change?.effectiveDate ?? null,
      reason: change?.reason ?? null,
    }),
  /** `clear_assignments` dönem başladıysa HER DURUMDA reddedilir; yalnız planlamada çalışır. */
  clear: (change?: EffectiveChangeInput): Promise<AssignmentBoard> =>
    call('clear_assignments', {
      effectiveDate: change?.effectiveDate ?? null,
      reason: change?.reason ?? null,
    }),
}
