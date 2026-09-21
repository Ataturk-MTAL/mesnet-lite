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

  /** Alan koordinatörlük ders yükü havuzu: Σ(haftalık ders saati × grup sayısı) + şeflerin planlama/bakım-onarım saatleri. */
  poolHours: number
  /** İşletmelere takdir edilen toplam saat. */
  awardedHours: number
  /** `poolHours - awardedHours`. Negatifse havuz aşılmış demektir; kırpılmaz. */
  remainingPoolHours: number

  companiesWithoutLocation: number
  studentsWithoutCompany: number
  companiesWithoutStudents: number
  companiesWithoutAssignment: number
}

export const dashboardApi = {
  get: (): Promise<DashboardStats> => call('get_dashboard_stats'),
}
