import { call } from './client'
import type { Company, CompanyRemoval, NewCompany } from '../types/models'

/** `preview_company_merge` yanıtı; hiçbir şey yazmadan birleşmenin etkisini gösterir. */
export interface CompanyMergePreview {
  fromName: string
  intoName: string
  students: { studentId: number; fullName: string }[]
  awardedHoursToClear: number
  endsCoordination: boolean
  warnings: string[]
}

/** `apply_company_merge` isteği. */
export interface CompanyMergeInput {
  fromCompanyId: number
  intoCompanyId: number
  effectiveDate: string | null
  reason: string
}

/** `apply_company_merge` yanıtı; kalıcı olarak yapılan işlemin özeti. */
export interface CompanyMergeSummary {
  movedStudents: number
  clearedHours: number
  endedCoordination: boolean
  warnings: string[]
}

export const companiesApi = {
  list: (): Promise<Company[]> => call('list_companies'),
  get: (id: number): Promise<Company> => call('get_company', { id }),
  create: (input: NewCompany): Promise<Company> => call('create_company', { input }),
  update: (id: number, input: NewCompany): Promise<Company> =>
    call('update_company', { id, input }),
  /** Geçmişi olan işletme silinmez, pasife alınır; `softDeleted` bunu ayırt eder. */
  remove: (id: number): Promise<CompanyRemoval> => call('delete_company', { id }),
  setLocation: (id: number, latitude: number, longitude: number): Promise<Company> =>
    call('set_company_location', { id, latitude, longitude }),
  /** Kaynaktaki öğrenciler hedefe taşınırsa ne olacağını, hiçbir şey yazmadan gösterir. */
  previewMerge: (fromCompanyId: number, intoCompanyId: number): Promise<CompanyMergePreview> =>
    call('preview_company_merge', { fromCompanyId, intoCompanyId }),
  /** Birleşmeyi kalıcı yazar: kaynak pasifleşir, öğrenciler hedefe taşınır, kaynağın saati sıfırlanır. */
  applyMerge: (input: CompanyMergeInput): Promise<CompanyMergeSummary> =>
    call('apply_company_merge', { ...input }),
}
