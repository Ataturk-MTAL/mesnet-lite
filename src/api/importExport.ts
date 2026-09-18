import { call } from './client'

/** Mevcut kayıtla çakışan işletme için uygulanacak politika. */
export type DuplicatePolicy = 'merge' | 'update' | 'skip'

export interface PreviewGroup {
  /** Normalize edilmiş ad; politika seçimleri bu anahtarla eşlenir. */
  key: string
  companyName: string
  addressText: string
  oneWayDistanceKm: number | null
  roundTripDistanceKm: number | null
  studentNames: string[]
  studentCount: number
  existingCompanyId: number | null
}

export interface ImportPreview {
  groups: PreviewGroup[]
  totalStudents: number
  duplicateCount: number
  errors: string[]
}

export interface ImportSummary {
  companiesCreated: number
  companiesMatched: number
  companiesUpdated: number
  companiesSkipped: number
  studentsCreated: number
  studentsSkipped: number
  errors: string[]
}

export const importApi = {
  preview: (content: string): Promise<ImportPreview> => call('preview_csv_import', { content }),
  apply: (content: string, policies: Record<string, DuplicatePolicy>): Promise<ImportSummary> =>
    call('apply_csv_import', { content, policies }),
}
