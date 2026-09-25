import type { DataTableFilterMeta } from 'openvue/datatable'

/**
 * DataTable'ın `update:filters` olayından gelen genel arama değerini metne
 * çevirir. OpenVue'nun filtre tipi `value: any` taşıdığından buradaki
 * `typeof` daraltması, `any`'nin çağıran koda sızmasını engeller.
 */
export function extractGlobalFilterValue(filterMeta: DataTableFilterMeta): string {
  const global = filterMeta.global
  if (typeof global !== 'object' || global === null || !('value' in global)) return ''
  const value = global.value
  return typeof value === 'string' ? value : ''
}

/** Store'daki arama metninden DataTable'ın beklediği `filters` nesnesini üretir. */
export function buildGlobalFilter(search: string): DataTableFilterMeta {
  return { global: { value: search || null, matchMode: 'contains' } }
}
