import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { useTermStore } from './term'
import type { TermWithDates } from '../types/models'

/**
 * Sidebar'daki "Tarihteki durum" seçicisinin paylaştığı tek kaynak. Seçili
 * tarih, varsayılan tarih (dönemin `defaultAsOf`'u) ve bunlardan türetilen
 * `isReadOnly` bayrağı. Dönem kapsamına sıkıştırma yalnız backend'de yaşar
 * (tek doğruluk kaynağı): varsayılan değer, backend'in dönem aralığına
 * sıkıştırıp gönderdiği `defaultAsOf` alanıdır. Planlama evresinde bu değer
 * bugün DEĞİL dönem başlangıcıdır; bu yüzden "salt okunur" kararı bugüne
 * değil, HER ZAMAN varsayılana göre verilir.
 */
export const useAsOfDateStore = defineStore('asOfDate', () => {
  const termStore = useTermStore()

  const asOfDate = ref('')
  /** Aktif dönemin varsayılan "tarihteki durum" tarihi (`TermWithDates.defaultAsOf`). */
  const defaultAsOf = ref('')
  /** Seçicinin izin verdiği aralık: dönem başı–sonu. */
  const termStartDate = ref('')
  const termEndDate = ref('')

  /** Seçili tarih varsayılandan farklıysa ekranlar salt okunur olur. */
  const isReadOnly = computed(() => asOfDate.value.length > 0 && asOfDate.value !== defaultAsOf.value)

  /**
   * Okuma komutlarına gidecek tarih. Varsayılan görünümde HER ZAMAN `null`
   * gider — bugünkü (düzenlenebilir) davranış korunur; yalnız salt okunur
   * durumda seçili tarih gönderilir.
   */
  const requestAsOf = computed<string | null>(() => (isReadOnly.value ? asOfDate.value : null))

  /** Aktif dönemin tarihleri yüklendiğinde ya da değiştiğinde çağrılır; seçici varsayılana döner. */
  function initializeFromTerm(term: TermWithDates): void {
    defaultAsOf.value = term.defaultAsOf
    termStartDate.value = term.startDate
    termEndDate.value = term.endDate
    asOfDate.value = term.defaultAsOf
  }

  /** Kullanıcı sidebar'dan elle bir tarih seçtiğinde çağrılır. */
  function setAsOfDate(date: string): void {
    if (date === asOfDate.value) return
    asOfDate.value = date
  }

  /**
   * "Bugün" düğmesi: gerçek takvim gününe değil, dönemin varsayılan tarihine
   * döner. Planlama evresinde gerçek bugün dönem aralığının DIŞINDA kalabilir;
   * bu yüzden tek doğru hedef her zaman `defaultAsOf`tur.
   */
  function goToToday(): void {
    setAsOfDate(defaultAsOf.value)
  }

  // Aktif dönemin tarihleri yüklenince ya da dönem değişince seçici varsayılana
  // ayarlanır. Bu bağlantı TEK bir yerde (burada) kurulur; ekranlar ya da
  // AppSidebar bunu ayrıca tetiklemez.
  watch(
    () => termStore.activeTermDates,
    (term) => {
      if (term) initializeFromTerm(term)
    },
    { immediate: true },
  )

  return {
    asOfDate,
    defaultAsOf,
    termStartDate,
    termEndDate,
    isReadOnly,
    requestAsOf,
    initializeFromTerm,
    setAsOfDate,
    goToToday,
  }
})
