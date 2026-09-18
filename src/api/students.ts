import { call } from './client'
import type { NewStudent, Student } from '../types/models'

export const studentsApi = {
  list: (): Promise<Student[]> => call('list_students'),
  create: (input: NewStudent): Promise<Student> => call('create_student', { input }),
  update: (id: number, input: NewStudent): Promise<Student> =>
    call('update_student', { id, input }),
  remove: (id: number): Promise<void> => call('delete_student', { id }),
}
