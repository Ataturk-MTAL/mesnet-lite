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
