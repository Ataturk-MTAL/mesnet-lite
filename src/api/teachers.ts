import { call } from './client'
import type { NewTeacher, Teacher, TeacherWithCapacity } from '../types/models'

export const teachersApi = {
  list: (): Promise<Teacher[]> => call('list_teachers'),
  /** Kapasite mevzuattan türetilir ve Rust tarafında hesaplanır. */
  listWithCapacity: (): Promise<TeacherWithCapacity[]> => call('list_teachers_with_capacity'),
  create: (input: NewTeacher): Promise<Teacher> => call('create_teacher', { input }),
  update: (id: number, input: NewTeacher): Promise<Teacher> =>
    call('update_teacher', { id, input }),
  remove: (id: number): Promise<void> => call('delete_teacher', { id }),
}
