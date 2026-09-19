// 'YYYY-MM-DD' tarih dizgisi ile OpenVue DatePicker'ın kullandığı yerel
// `Date` nesnesi arasında dönüşüm yapan saf yardımcılar. Takvim günü yerel
// yıl/ay/gün bileşenlerinden kurulur; UTC'ye çevrilmediği için gün kayması
// olmaz. `EffectiveDateField` ve `AsOfDatePicker` bu modülü paylaşır.

/** DatePicker'ın tekil/çoklu/aralık seçim modlarıyla paylaştığı v-model tipi. */
export type DatePickerModelValue = Date | Date[] | (Date | null)[] | null

/** 'YYYY-MM-DD' dizgisini yerel `Date`'e çevirir; boş ya da biçimsiz girdide `null` döner. */
export function isoToDate(iso: string | null): Date | null {
  if (!iso) return null
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso)
  if (!match) return null
  const [, yearText, monthText, dayText] = match
  return new Date(Number(yearText), Number(monthText) - 1, Number(dayText))
}

/** DatePicker'dan gelen değeri 'YYYY-MM-DD' dizgisine çevirir; tekil `Date` dışındaki her şey `null` olur. */
export function dateToIso(value: DatePickerModelValue): string | null {
  if (!(value instanceof Date)) return null
  const year = value.getFullYear()
  const month = String(value.getMonth() + 1).padStart(2, '0')
  const day = String(value.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}
