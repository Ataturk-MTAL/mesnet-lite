import { call } from './client'

/**
 * Ders Yükü ekranındaki tek bir satır — ister kayıtlı, ister öğrenci
 * kayıtlarından türeyen bir öneri.
 */
export interface TeachingLoadRow {
  /** Kayıtlı satırın kimliği. Öneri satırında `null`. */
  id: number | null
  grade: string
  branch: string
  weeklyHours: number
  /** Etkin grup sayısı: elle girilmişse o değer, değilse tabloya göre hesaplanan. */
  groupCount: number
  /** Öğrenci sayısından Norm Kadro Yön. md. 22 tablosuyla hesaplanan grup sayısı. */
  autoGroupCount: number
  /** `true` ise grup sayısı kullanıcı tarafından elle girilmiştir ve korunur. */
  isGroupManual: boolean
  /**
   * `true` ise bu satır o dönemin ÖĞRENCİ kayıtlarından türetildi, henüz
   * kaydedilmedi (`id === null`, `weeklyHours`/`groupCount` sıfır).
   */
  isSuggested: boolean
}

/** Ders Yükü ekranının tamamı: satırlar + o dönemin havuzu. */
export interface TeachingLoadBoard {
  term: string
  rows: TeachingLoadRow[]
  /** Σ (haftalık ders saati × grup sayısı) — yalnızca KAYITLI satırlardan. */
  branchHours: number
  /**
   * Alanın şeflerinin planlama-bakım-onarım saatleri toplamı
   * (alan şefi 10, atölye/laboratuvar şefi 6).
   */
  chiefPlanningHours: number
  /** Toplam havuz: `branchHours + chiefPlanningHours`. */
  poolHours: number
}

/** Kaydetmeye giden satır. */
export interface TeachingLoadInput {
  grade: string
  branch: string
  weeklyHours: number
  groupCount: number
  /** `false` ise sunucu `groupCount`'u yok sayar ve kendi hesabını kullanır. */
  isGroupManual: boolean
}

export const teachingLoadApi = {
  get: (): Promise<TeachingLoadBoard> => call('get_teaching_load_board'),
  /** Aktif dönemin TÜM satırlarını değiştirir; kısmi güncelleme yoktur. */
  save: (rows: TeachingLoadInput[]): Promise<TeachingLoadBoard> =>
    call('save_teaching_load', { rows }),
}
