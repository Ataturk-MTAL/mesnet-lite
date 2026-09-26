import { call } from './client'
import type { EffectiveChangeInput } from '../types/models'

/** Atama ekranındaki işletme kartı. */
export interface BoardCompany {
  companyId: number
  companyName: string
  addressText: string
  /** Gruplama anahtarı: atanmamış işletme listesi bu alana göre gruplanır. Ayrıştırılamayan adreste boş dize. */
  district: string
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

export interface ProposedAssignment {
  companyId: number
  companyName: string
  teacherId: number
  teacherName: string
  awardedHours: number
  visitDay: number
  visitHour: number
  exactBranchMatch: boolean
}

export interface UnassignedCompany {
  companyId: number
  companyName: string
  /** Türkçe gerekçe. */
  reason: string
}

export interface AllocationProposal {
  assignments: ProposedAssignment[]
  unassigned: UnassignedCompany[]
}

export const assignmentsApi = {
  /** `asOf` `null` ise güncel kayıt (düzenlenebilir); bir tarihse o güne göre okuma (salt okunur). */
  get: (asOf: string | null = null): Promise<AssignmentBoard> =>
    call('get_assignment_board', { asOf }),
  /** Öneri üretir; hiçbir şey kaydetmez. */
  propose: (): Promise<AllocationProposal> => call('propose_assignments'),
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
