import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import type { TermWithDates } from '../types/models'

/**
 * Türkiye'nin sabit UTC+3 takvim günü. Backend `today_local()` ile aynı
 * hesabı yapar (bkz. Genel Kısıtlar); sistem saat dilimine bağlı kalınmaz.
 */
function todayLocalIso(): string {
  const turkeyMs = Date.now() + 3 * 60 * 60 * 1000
  return new Date(turkeyMs).toISOString().slice(0, 10)
}

/**
 * Sidebar'daki "Tarihteki durum" seçicisinin paylaştığı tek kaynak. Seçili
 * "tarihteki durum" tarihi ve ondan türetilen `isToday` / `isReadOnly`
 * bayrakları. Dönem kapsamına sıkıştırma yalnız backend'de yaşar (tek
 * doğruluk kaynağı): varsayılan değer, backend'in dönem aralığına sıkıştırıp
 * gönderdiği `defaultAsOf` alanıdır.
 */
export const useAsOfDateStore = defineStore('asOfDate', () => {
  const asOfDate = ref('')

  const isToday = computed(() => asOfDate.value === todayLocalIso())
  const isReadOnly = computed(() => asOfDate.value.length > 0 && !isToday.value)

  /** Dönem yüklendiğinde ya da değiştirildiğinde çağrılır. */
  function initializeFromTerm(term: TermWithDates): void {
    if (asOfDate.value === term.defaultAsOf) return
    asOfDate.value = term.defaultAsOf
  }

  /** Kullanıcı sidebar'dan elle bir tarih seçtiğinde çağrılır. */
  function setAsOfDate(date: string): void {
    if (date === asOfDate.value) return
    asOfDate.value = date
  }

  /** "Bugün" düğmesi: `isToday` ile aynı gün hesabını kullanır, sapma olmaz. */
  function goToToday(): void {
    setAsOfDate(todayLocalIso())
  }

  return { asOfDate, isToday, isReadOnly, initializeFromTerm, setAsOfDate, goToToday }
})
