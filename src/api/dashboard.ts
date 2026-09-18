import { call } from './client'

export interface DashboardStats {
  /** Sayıların ait olduğu eğitim-öğretim yılı. */
  term: string

  companyCount: number
  studentCount: number
  teacherCount: number
  activeTeacherCount: number

  /** Aktif öğretmenlerin koordinatörlük kapasiteleri toplamı. */
  totalCapacityHours: number
  /** Atamalarda takdir edilmiş toplam saat. */
  assignedHours: number
  /** Kalan. Negatifse kapasite aşılmış demektir. */
  remainingHours: number
  teachersAtCapacity: number
  teachersOverCapacity: number

  companiesWithoutLocation: number
  studentsWithoutCompany: number
  companiesWithoutStudents: number
  companiesWithoutAssignment: number
}

export const dashboardApi = {
  get: (): Promise<DashboardStats> => call('get_dashboard_stats'),
}
