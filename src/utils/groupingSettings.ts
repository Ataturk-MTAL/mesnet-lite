import type { SettingsMap } from '../api/settings'

export type GroupingMode = 'distance' | 'manual'

export const GROUPING_MIN_DIAMETER_KM = 0.5
export const GROUPING_MAX_DIAMETER_KM = 50
export const GROUPING_DIAMETER_STEP_KM = 0.5
export const GROUPING_DEFAULT_DIAMETER_KM = 3
export const GROUPING_MAX_NAME_LENGTH = 80

export interface ManualGroupDraft {
  name: string
  neighborhoods: string[]
  districts: string[]
}

export interface GroupingDraft {
  mode: GroupingMode
  /** Girişte boşaltılmış alan `null`. */
  maxDiameterKm: number | null
  groups: ManualGroupDraft[]
}

export type GroupNameError = 'empty' | 'duplicate' | 'tooLong'

export interface GroupingValidation {
  isDiameterValid: boolean
  /** `groups` ile aynı sırada; hatasız grup için `null`. */
  nameErrors: (GroupNameError | null)[]
  isValid: boolean
}

export function defaultGrouping(): GroupingDraft {
  return { mode: 'distance', maxDiameterKm: GROUPING_DEFAULT_DIAMETER_KM, groups: [] }
}

function normalizeKey(value: string): string {
  return value.trim().toLocaleLowerCase('tr')
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : []
}

function parseGroups(raw: string | undefined): ManualGroupDraft[] {
  if (!raw || raw.trim().length === 0) return []
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    return []
  }
  if (!Array.isArray(parsed)) return []
  return parsed.flatMap((item: unknown): ManualGroupDraft[] => {
    if (typeof item !== 'object' || item === null) return []
    const record = item as Record<string, unknown>
    return [
      {
        name: typeof record.name === 'string' ? record.name : '',
        neighborhoods: asStringArray(record.neighborhoods),
        districts: asStringArray(record.districts),
      },
    ]
  })
}

function parseDiameter(raw: string | undefined): number {
  // Arka uç noktalı yazar; elle düzenlenmiş değerde virgül gelirse de okunur.
  const parsed = Number.parseFloat((raw ?? '').replace(',', '.'))
  return Number.isNaN(parsed) ? GROUPING_DEFAULT_DIAMETER_KM : parsed
}

/** Ayar haritasından taslak üretir; eksik/bozuk değerde varsayılana düşer. */
export function parseGroupingSettings(settings: SettingsMap): GroupingDraft {
  return {
    mode: settings.grouping_mode === 'manual' ? 'manual' : 'distance',
    maxDiameterKm: parseDiameter(settings.grouping_max_diameter_km),
    groups: parseGroups(settings.grouping_manual_groups),
  }
}

export function validateGrouping(draft: GroupingDraft): GroupingValidation {
  const diameter = draft.maxDiameterKm
  const isDiameterValid =
    diameter !== null && diameter >= GROUPING_MIN_DIAMETER_KM && diameter <= GROUPING_MAX_DIAMETER_KM

  const seen = new Set<string>()
  const nameErrors = draft.groups.map((group): GroupNameError | null => {
    const trimmed = group.name.trim()
    if (trimmed.length === 0) return 'empty'
    if (trimmed.length > GROUPING_MAX_NAME_LENGTH) return 'tooLong'
    const key = normalizeKey(trimmed)
    if (seen.has(key)) return 'duplicate'
    seen.add(key)
    return null
  })

  return {
    isDiameterValid,
    nameErrors,
    isValid: isDiameterValid && nameErrors.every((error) => error === null),
  }
}

/** Aynı değeri (Türkçe büyük/küçük harf duyarsız) bir kez tutar, boşları atar. */
export function uniqueValues(values: string[]): string[] {
  const seen = new Set<string>()
  const result: string[] = []
  for (const value of values) {
    const trimmed = value.trim()
    const key = normalizeKey(trimmed)
    if (key.length === 0 || seen.has(key)) continue
    seen.add(key)
    result.push(trimmed)
  }
  return result
}

/**
 * Kaydedilecek ayar girdileri. Çap her zaman NOKTA ayraçlı yazılır (arka uç
 * "3,5"i reddeder); `String(number)` yerel ayardan bağımsız noktalı üretir.
 * Geçersiz taslakla çağrılmamalıdır; önce `validateGrouping`.
 */
export function serializeGroupingSettings(draft: GroupingDraft): SettingsMap {
  const groups = draft.groups.map((group) => ({
    name: group.name.trim(),
    neighborhoods: uniqueValues(group.neighborhoods),
    districts: uniqueValues(group.districts),
  }))
  return {
    grouping_mode: draft.mode,
    grouping_max_diameter_km: String(draft.maxDiameterKm ?? GROUPING_DEFAULT_DIAMETER_KM),
    grouping_manual_groups: JSON.stringify(groups),
  }
}
