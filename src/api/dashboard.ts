import { call } from './client'

export interface DashboardStats {
  companyCount: number
  studentCount: number
  teacherCount: number
  activeTeacherCount: number
  companiesWithoutLocation: number
  studentsWithoutCompany: number
  companiesWithoutStudents: number
}

export const dashboardApi = {
  get: (): Promise<DashboardStats> => call('get_dashboard_stats'),
}
