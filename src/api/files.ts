import { call } from './client'

export interface GeocodeSummary {
  resolved: number
  failed: number
  skipped: number
  warnings: string[]
}

/**
 * Üretilen baytlar diske Rust tarafında yazılır; webview içinden `<a download>`
 * ile indirme Tauri'de güvenilir değildir.
 */
async function saveToDownloads(fileName: string, bytes: number[]): Promise<string> {
  return call<string>('save_to_downloads', { fileName, bytes })
}

export const filesApi = {
  /**
   * Excel çalışma kitabını üretir ve indirilenler klasörüne kaydeder.
   * `versionId` verilmezse güncel veriden üretilir ve otomatik bir sürüm
   * kaydedilir; bir sürüm numarası verilirse o sürümden üretilir ve yeni
   * sürüm kaydedilmez.
   */
  exportWorkbook: async (fileName: string, versionId: number | null = null): Promise<string> => {
    const bytes = await call<number[]>('export_workbook', { versionId })
    return saveToDownloads(fileName, bytes)
  },
  /** Koordinatör Görevlendirme Çizelgesi'ni PDF üretir ve kaydeder. */
  exportAssignmentSheet: async (fileName: string, versionId: number | null = null): Promise<string> => {
    const bytes = await call<number[]>('export_assignment_sheet', { versionId })
    return saveToDownloads(fileName, bytes)
  },
  /** Öğretmen Ziyaret Listeleri'ni PDF üretir ve kaydeder. */
  exportVisitLists: async (fileName: string, versionId: number | null = null): Promise<string> => {
    const bytes = await call<number[]>('export_visit_lists', { versionId })
    return saveToDownloads(fileName, bytes)
  },
  /** İşletme Belirleme Komisyon Tutanağı'nı PDF üretir ve kaydeder. */
  exportCommissionMinutesPdf: async (fileName: string, versionId: number | null = null): Promise<string> => {
    const bytes = await call<number[]>('export_commission_minutes_pdf', { versionId })
    return saveToDownloads(fileName, bytes)
  },
  /** İşletme Belirleme Komisyon Tutanağı'nı Excel olarak üretir ve kaydeder. */
  exportCommissionMinutesXlsx: async (fileName: string, versionId: number | null = null): Promise<string> => {
    const bytes = await call<number[]>('export_commission_minutes_xlsx', { versionId })
    return saveToDownloads(fileName, bytes)
  },
  /** Konumu olmayan işletmeleri Nominatim ile çözer. Saniyede bir istek. */
  geocodePending: (): Promise<GeocodeSummary> => call('geocode_pending_companies'),
}
