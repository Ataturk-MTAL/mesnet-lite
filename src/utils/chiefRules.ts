import { labels } from '../i18n/labels'
import type { ChiefType, TeacherWithCapacity } from '../types/models'

/** MADDE 6/1-a — müdür ve müdür yardımcılarına haftada verilebilecek en fazla ek ders. */
export const PRINCIPAL_MAX_EXTRA_HOURS = 6

/** MADDE 5/1-a — müdür ve müdür yardımcılarının aylık karşılığı ders saati. */
export const PRINCIPAL_BASE_HOURS = 6

/** MADDE 6/4 — unvana göre şeflik saati; müdür ve müdür yardımcısında yoktur. */
export const CHIEF_HOURS_BY_TYPE: Readonly<Record<ChiefType, number>> = {
  none: 0,
  workshop_lab: 6,
  department: 10,
  deputy_principal: 0,
  principal: 0,
}

/** Rütbe sırası: küçük değer daha üst rütbe. */
export const CHIEF_RANK: Readonly<Record<ChiefType, number>> = {
  principal: 0,
  deputy_principal: 1,
  department: 2,
  workshop_lab: 3,
  none: 4,
}

export interface ChiefOption {
  value: ChiefType
  label: string
}

/** Seçim listesi sırası: Öğretmen, Atölye/Lab. Şefi, Bölüm Şefi, Müdür Yardımcısı, Müdür. */
export const CHIEF_OPTIONS: ChiefOption[] = [
  { value: 'none', label: labels.chiefType.none },
  { value: 'workshop_lab', label: labels.chiefType.workshop_lab },
  { value: 'department', label: labels.chiefType.department },
  { value: 'deputy_principal', label: labels.chiefType.deputy_principal },
  { value: 'principal', label: labels.chiefType.principal },
]

export function isPrincipalType(chiefType: ChiefType): boolean {
  return chiefType === 'principal' || chiefType === 'deputy_principal'
}

/** Müdür/müdür yardımcısında azami ek ders 6'yı aşıyorsa uyarı metni, yoksa null. */
export function principalCapWarning(chiefType: ChiefType, maxExtraHours: number): string | null {
  return isPrincipalType(chiefType) && maxExtraHours > PRINCIPAL_MAX_EXTRA_HOURS
    ? labels.teacher.principalCapWarning
    : null
}

/** Yeni unvana geçilince ayarlanacak saatler; müdür kadrosu dışında null (dokunulmaz). */
export function principalLoadDefaults(
  chiefType: ChiefType,
): { baseHours: number; maxExtraHours: number } | null {
  return isPrincipalType(chiefType)
    ? { baseHours: PRINCIPAL_BASE_HOURS, maxExtraHours: PRINCIPAL_MAX_EXTRA_HOURS }
    : null
}

/**
 * Metinleri Türkçe harf sırasına göre sıralar ve her metne sıra numarası verir.
 * Aynı metin aynı numarayı alır. DataTable'ın kendi karşılaştırıcısı çalışma
 * zamanı yerelini kullanır; sayısal anahtar Türkçe sırayı yerelden bağımsız kılar.
 */
export function turkishSortKeys(values: readonly string[]): Map<string, number> {
  const distinct = [...new Set(values)].sort((a, b) => a.localeCompare(b, 'tr'))
  return new Map(distinct.map((value, index) => [value, index]))
}

export type TeacherRow = TeacherWithCapacity & {
  chiefRank: number
  firstNameSortKey: number
  lastNameSortKey: number
}

/** Tabloya verilecek satırlar: özel sıralama alanları eklenmiş yeni nesneler. */
export function buildTeacherRows(teachers: readonly TeacherWithCapacity[]): TeacherRow[] {
  const firstKeys = turkishSortKeys(teachers.map((t) => t.firstName))
  const lastKeys = turkishSortKeys(teachers.map((t) => t.lastName))
  return teachers.map((teacher) => ({
    ...teacher,
    chiefRank: CHIEF_RANK[teacher.chiefType],
    firstNameSortKey: firstKeys.get(teacher.firstName) ?? 0,
    lastNameSortKey: lastKeys.get(teacher.lastName) ?? 0,
  }))
}
