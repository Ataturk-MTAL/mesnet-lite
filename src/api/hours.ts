import { call } from './client'

/** MESNET'in `İşletme Saat Ayarları` tablosunun bir satırı. */
export interface HoursRow {
  companyId: number
  companyName: string
  addressText: string
  /** CSV'den gelen tek yön yol mesafesi. */
  oneWayDistanceKm: number | null
  /** Saat kuralında kullanılan değer: tek yönün iki katı. */
  roundTripDistanceKm: number | null
  studentCount: number
  /** `Verilebilir Maks.` — kural bulunamazsa null. */
  maxHours: number | null
  awardedHours: number
  isHonorary: boolean
  isLocked: boolean
  notes: string
  isSaved: boolean
}

export interface HoursBoard {
  term: string
  rows: HoursRow[]
  poolHours: number
  totalAwarded: number
  totalMax: number
  honoraryCount: number
  lockedCount: number
  withoutRuleCount: number
  warnings: string[]
}

/** Kaydetmeye giden satır. */
export interface HoursInput {
  companyId: number
  maxHoursSnapshot: number
  awardedHours: number
  isHonorary: boolean
  isLocked: boolean
  notes: string
}

export interface AutoDistributeRow {
  companyId: number
  maxHours: number
  studentCount: number
  isLocked: boolean
  currentAwarded: number
  isHonorary: boolean
}

export interface DistributionResult {
  companyId: number
  awardedHours: number
  isHonorary: boolean
  wasLocked: boolean
}

export interface DistributionOutcome {
  results: DistributionResult[]
  lockedHours: number
  distributedHours: number
  leftoverHours: number
  honoraryCount: number
  warnings: string[]
}

export const hoursApi = {
  get: (): Promise<HoursBoard> => call('get_hours_board'),
  save: (rows: HoursInput[]): Promise<HoursBoard> => call('save_company_hours', { rows }),
  /** Öneri üretir; kaydetmez. */
  autoDistribute: (rows: AutoDistributeRow[]): Promise<DistributionOutcome> =>
    call('auto_distribute_hours', { rows }),
}
