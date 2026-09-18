import { call } from './client'

/** Atama ekranındaki işletme kartı. */
export interface BoardCompany {
  companyId: number
  companyName: string
  addressText: string
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

export const assignmentsApi = {
  get: (): Promise<AssignmentBoard> => call('get_assignment_board'),
  assign: (input: NewAssignment): Promise<AssignmentBoard> => call('assign_company', { input }),
  unassign: (companyId: number): Promise<AssignmentBoard> =>
    call('unassign_company', { companyId }),
  clear: (): Promise<AssignmentBoard> => call('clear_assignments'),
}
