import { computed, ref } from 'vue'
import type { TermWithDates } from '../types/models'

/**
 * Sidebar'daki "Tarihteki durum" seçicisinin paylaştığı tek kaynak. Modül
 * düzeyinde tanımlanır; hangi bileşen içe aktarırsa aktarsın aynı `ref`'i
 * paylaşır, böylece seçim tüm ekranlarda tutarlı kalır.
 */
const asOfDateRef = ref<string>('')

/**
 * Türkiye'nin sabit UTC+3 takvim günü. Backend `today_local()` ile aynı
 * hesabı yapar (bkz. Genel Kısıtlar); sistem saat dilimine bağlı kalınmaz.
 */
function todayLocalIso(): string {
  const turkeyMs = Date.now() + 3 * 60 * 60 * 1000
  return new Date(turkeyMs).toISOString().slice(0, 10)
}

/**
 * Seçili "tarihteki durum" tarihi ve ondan türetilen `isToday` / `isReadOnly`
 * bayrakları. Dönem kapsamına sıkıştırma yalnız backend'de yaşar (tek
 * doğruluk kaynağı): varsayılan değer, backend'in dönem aralığına sıkıştırıp
 * gönderdiği `defaultAsOf` alanıdır.
 */
export function useAsOfDate() {
  const isToday = computed(() => asOfDateRef.value === todayLocalIso())
  const isReadOnly = computed(() => asOfDateRef.value.length > 0 && !isToday.value)

  /** Dönem yüklendiğinde ya da değiştirildiğinde çağrılır. */
  function initializeFromTerm(term: TermWithDates): void {
    if (asOfDateRef.value === term.defaultAsOf) return
    asOfDateRef.value = term.defaultAsOf
  }

  /** Kullanıcı sidebar'dan elle bir tarih seçtiğinde çağrılır. */
  function setAsOfDate(date: string): void {
    if (date === asOfDateRef.value) return
    asOfDateRef.value = date
  }

  return { asOfDate: asOfDateRef, isToday, isReadOnly, initializeFromTerm, setAsOfDate }
}
