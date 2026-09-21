import { call } from './client'
import type { NewTeacher, Teacher, TeacherWithCapacity } from '../types/models'

export const teachersApi = {
  list: (): Promise<Teacher[]> => call('list_teachers'),
  /** Kapasite mevzuattan türetilir ve Rust tarafında hesaplanır. */
  listWithCapacity: (): Promise<TeacherWithCapacity[]> => call('list_teachers_with_capacity'),
  /**
   * `effectiveDate`/`reason` yalnız dönem başladıktan sonra yük veya şeflik
   * değişen kayıtlarda zorunludur (tarihçe kapısı); aksi hâlde `null`
   * gönderilir. Planlama evresinde veya yalnız kimlik alanları değişiyorsa
   * çağıran taraf `null` geçer.
   */
  create: (
    input: NewTeacher,
    effectiveDate: string | null = null,
    reason: string | null = null,
  ): Promise<Teacher> => call('create_teacher', { input, effectiveDate, reason }),
  update: (
    id: number,
    input: NewTeacher,
    effectiveDate: string | null = null,
    reason: string | null = null,
  ): Promise<Teacher> => call('update_teacher', { id, input, effectiveDate, reason }),
  remove: (id: number): Promise<void> => call('delete_teacher', { id }),
}
