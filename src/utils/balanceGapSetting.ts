import type { SettingsMap } from '../api/settings'

/** Ayar anahtarı: öğretmenler arası saat farkı eşiği (saat). */
export const BALANCE_GAP_KEY = 'allocation_balance_gap_hours'
export const BALANCE_GAP_MIN = 0
export const BALANCE_GAP_MAX = 40
export const BALANCE_GAP_DEFAULT = 4

/** Kayıtlı değeri okur; yok ya da geçersizse varsayılan (4). */
export function parseBalanceGap(settings: SettingsMap): number {
  const raw = settings[BALANCE_GAP_KEY]
  if (raw === undefined || !/^\d+$/.test(raw.trim())) return BALANCE_GAP_DEFAULT
  const parsed = Number.parseInt(raw, 10)
  return isBalanceGapValid(parsed) ? parsed : BALANCE_GAP_DEFAULT
}

/** Girişte boşaltılmış alan `null`; yalnız 0–40 arası tam sayı geçerlidir. */
export function isBalanceGapValid(value: number | null): value is number {
  return value !== null && Number.isInteger(value) && value >= BALANCE_GAP_MIN && value <= BALANCE_GAP_MAX
}

/** Kaydedilecek ayar girdisi. */
export function serializeBalanceGap(value: number): SettingsMap {
  return { [BALANCE_GAP_KEY]: String(value) }
}
