import { call } from './client'
import type { NewTeacher, NewTeacherProfile, Teacher, TeacherWithCapacity } from '../types/models'

export const teachersApi = {
  list: (): Promise<Teacher[]> => call('list_teachers'),
  /**
   * Kapasite mevzuattan türetilir ve Rust tarafında hesaplanır. `asOf` `null`
   * ise güncel kayıt (düzenlenebilir); bir tarihse o güne göre okuma (salt okunur).
   */
  listWithCapacity: (asOf: string | null = null): Promise<TeacherWithCapacity[]> =>
    call('list_teachers_with_capacity', { asOf }),
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
  /** Yalnız profil alanları; yük alanları `TeacherLoadDialog` (tarihçe) yolundan değişir. */
  update: (id: number, input: NewTeacherProfile): Promise<Teacher> =>
    call('update_teacher', { id, input }),
  remove: (id: number): Promise<void> => call('delete_teacher', { id }),
}
