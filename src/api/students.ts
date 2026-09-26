import { call } from './client'
import type { NewStudent, Student } from '../types/models'

export const studentsApi = {
  /**
   * Aktif eğitim-öğretim yılındaki öğrenciler. `asOf` `null` ise güncel kayıt
   * (düzenlenebilir); bir tarihse o güne göre okuma (salt okunur).
   */
  list: (asOf: string | null = null): Promise<Student[]> => call('list_students', { asOf }),
  /** Veritabanındaki tüm dönemler, en yeniden eskiye. */
  listTerms: (): Promise<string[]> => call('list_terms'),
  create: (input: NewStudent): Promise<Student> => call('create_student', { input }),
  update: (id: number, input: NewStudent): Promise<Student> =>
    call('update_student', { id, input }),
  remove: (id: number): Promise<void> => call('delete_student', { id }),
}
