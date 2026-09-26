import { call } from './client'

export interface AvailabilityTeacher {
  teacherId: number
  teacherName: string
  /** Boş saatler: `{gün}-{saat}` anahtarları. */
  freeSlots: string[]
  freeCount: number
}

export interface ClassDays {
  grade: string
  days: number[]
  studentCount: number
}

export interface AvailabilityBoard {
  term: string
  teachers: AvailabilityTeacher[]
  classes: ClassDays[]
  dayStartHour: number
  dayEndHour: number
  otherTerms: string[]
  warnings: string[]
}

export interface SlotInput {
  dayOfWeek: number
  hour: number
}

export interface CopyOutcome {
  availabilityCopied: boolean
  classDaysCopied: boolean
  board: AvailabilityBoard
}

export const availabilityApi = {
  /** `asOf` `null` ise güncel kayıt (düzenlenebilir); bir tarihse o güne göre okuma (salt okunur). */
  get: (asOf: string | null = null): Promise<AvailabilityBoard> =>
    call('get_availability_board', { asOf }),
  /** Haftanın tamamını gönderir; kısmi güncelleme yoktur. */
  saveTeacher: (teacherId: number, slots: SlotInput[]): Promise<AvailabilityBoard> =>
    call('save_teacher_availability', { teacherId, slots }),
  saveClassDays: (grade: string, days: number[]): Promise<AvailabilityBoard> =>
    call('save_class_days', { grade, days }),
  copyFromTerm: (fromTerm: string): Promise<CopyOutcome> =>
    call('copy_schedule_from_term', { fromTerm }),
}
