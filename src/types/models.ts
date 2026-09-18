// Rust tarafındaki serde camelCase çıktısının birebir karşılığı.

/** Harita üzerinde bir nokta. */
export interface LatLng {
  latitude: number
  longitude: number
}

export type GeocodeStatus = 'pending' | 'resolved' | 'failed' | 'manual'
export type ChiefType = 'none' | 'workshop_lab' | 'department'
export type EmploymentType = 'tenured' | 'contracted'

export interface Company {
  id: number
  name: string
  contactFirstName: string
  contactLastName: string
  phone: string
  email: string
  addressText: string
  latitude: number | null
  longitude: number | null
  geocodeStatus: GeocodeStatus
  /** TEK YÖN yol mesafesi. Saat tavanı kuralları bunun iki katını kullanır. */
  oneWayDistanceKm: number | null
  notes: string
  createdAt: string
  updatedAt: string
}

export interface NewCompany {
  name: string
  contactFirstName: string
  contactLastName: string
  phone: string
  email: string
  addressText: string
  latitude: number | null
  longitude: number | null
  oneWayDistanceKm: number | null
  notes: string
}

/** Saat tavanı kurallarında kullanılan gidiş-dönüş mesafesi. */
export function roundTripDistanceKm(company: Company): number | null {
  return company.oneWayDistanceKm === null ? null : company.oneWayDistanceKm * 2
}

export interface Teacher {
  id: number
  firstName: string
  lastName: string
  registryNo: string
  field: string
  /** JSON dizi metni olarak saklanır; okumak için parseBranches kullanın. */
  branches: string
  employmentType: EmploymentType
  baseHours: number
  maxExtraHours: number
  otherExtraHours: number
  chiefType: ChiefType
  /** SQLite'ta boolean yoktur; 0 veya 1. */
  isActive: number
}

/** Öğretmen + mevzuattan türetilen kapasite. Kapasite Rust tarafında hesaplanır. */
export interface TeacherWithCapacity extends Teacher {
  chiefHours: number
  statutoryCap: number
  capacity: number
}

export interface NewTeacher {
  firstName: string
  lastName: string
  registryNo: string
  field: string
  branches: string[]
  employmentType: EmploymentType
  baseHours: number
  maxExtraHours: number
  otherExtraHours: number
  chiefType: ChiefType
  isActive: boolean
}

export interface NewStudent {
  firstName: string
  lastName: string
  studentNo: string | null
  grade: string
  branch: string
  companyId: number | null
  submittedAt: string | null
}

export interface Student {
  id: number
  firstName: string
  lastName: string
  studentNo: string | null
  grade: string
  branch: string
  companyId: number | null
  submittedAt: string | null
}

/** `branches` bozuk JSON içerse bile okuma yolu düşmemeli. */
export function parseBranches(raw: string): string[] {
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((b): b is string => typeof b === 'string') : []
  } catch {
    return []
  }
}
