import { call } from './client'

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
  get: (): Promise<AssignmentBoard> => call('get_assignment_board'),
  /** Öneri üretir; hiçbir şey kaydetmez. */
  propose: (): Promise<AllocationProposal> => call('propose_assignments'),
  assign: (input: NewAssignment): Promise<AssignmentBoard> => call('assign_company', { input }),
  unassign: (companyId: number): Promise<AssignmentBoard> =>
    call('unassign_company', { companyId }),
  clear: (): Promise<AssignmentBoard> => call('clear_assignments'),
}
