import { call } from './client'

/** Rust'a gönderilecek dosya; içerik bayt dizisi olarak taşınır (metin OKUNMAZ). */
export interface StudentListFile {
  name: string
  content: number[]
}

/** Bir satırın önizlemedeki durumu. */
export type StudentListRowStatus = 'new' | 'unchanged' | 'changed'

/** `changed` durumundaki bir öğrencinin içe aktarımdan önceki değerleri. */
export interface StudentListPreviousValue {
  firstName: string
  lastName: string
  grade: string
  branch: string
}

export interface StudentListRowPreview {
  /** e-Okul dosyasında öğrenci no sütunu boş bırakılabildiği için `Option<String>` olarak gelir. */
  studentNo: string | null
  firstName: string
  lastName: string
  branch: string
  status: StudentListRowStatus
  previous: StudentListPreviousValue | null
}

/** Tek bir dosyanın (bir fiziksel sınıfın) önizlemesi. */
export interface StudentListClassPreview {
  fileName: string
  grade: string
  fieldName: string
  rows: StudentListRowPreview[]
  newCount: number
  unchangedCount: number
  changedCount: number
}

export interface StudentListPreview {
  classes: StudentListClassPreview[]
  warnings: string[]
}

export interface StudentListSummary {
  created: number
  updated: number
  skipped: number
  warnings: string[]
}

/**
 * Seçilen dosyayı Rust'a gönderilecek bayt dizisine çevirir. Dosya İKİLİDİR
 * (e-Okul .XLS); metin olarak okumak içeriği bozar.
 */
export async function readFileAsBytes(file: File): Promise<StudentListFile> {
  const buffer = await file.arrayBuffer()
  return { name: file.name, content: Array.from(new Uint8Array(buffer)) }
}

export const studentListImportApi = {
  preview: (files: StudentListFile[]): Promise<StudentListPreview> =>
    call('preview_student_list_import', { files }),
  apply: (files: StudentListFile[], effectiveDate: string | null, reason: string): Promise<StudentListSummary> =>
    call('apply_student_list_import', { files, effectiveDate, reason }),
}
