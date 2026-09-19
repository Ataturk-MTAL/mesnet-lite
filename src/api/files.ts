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
  /** Excel çalışma kitabını üretir ve indirilenler klasörüne kaydeder. */
  exportWorkbook: async (fileName: string): Promise<string> => {
    const bytes = await call<number[]>('export_workbook')
    return saveToDownloads(fileName, bytes)
  },
  /** Koordinatör Görevlendirme Çizelgesi'ni PDF üretir ve kaydeder. */
  exportAssignmentSheet: async (fileName: string): Promise<string> => {
    const bytes = await call<number[]>('export_assignment_sheet')
    return saveToDownloads(fileName, bytes)
  },
  /** Öğretmen Ziyaret Listeleri'ni PDF üretir ve kaydeder. */
  exportVisitLists: async (fileName: string): Promise<string> => {
    const bytes = await call<number[]>('export_visit_lists')
    return saveToDownloads(fileName, bytes)
  },
  /** Konumu olmayan işletmeleri Nominatim ile çözer. Saniyede bir istek. */
  geocodePending: (): Promise<GeocodeSummary> => call('geocode_pending_companies'),
}
