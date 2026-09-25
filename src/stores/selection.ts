import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Stream } from '../types/models'

/**
 * Ekranlar arası paylaşılan seçim durumu (öğretmen seçimi, tarihçe filtreleri,
 * arama kutuları). `<RouterView>` sayfa değişince bileşeni söker; bileşene ait
 * `ref` o anda kaybolur. Bu store bileşen ömründen bağımsız yaşadığı için
 * kullanıcı sayfalar arasında geçiş yaptığında seçimler sıfırlanmaz.
 *
 * Yalnızca bellekte tutulur — `localStorage`/`sessionStorage` KULLANILMAZ
 * (bkz. `useAuth.ts`). Oturum kapanınca `reset()` ile temizlenir; başka bir
 * kullanıcı öncekinin filtrelerini devralmaz (bkz. `App.vue`).
 */
export const useSelectionStore = defineStore('selection', () => {
  // Dağıtım (Allocation) ve Müsaitlik (Availability) ekranlarının ORTAK öğretmen seçimi.
  const selectedTeacherId = ref<number | null>(null)

  // Tarihçe (History) ekranının filtreleri.
  const historyStream = ref<Stream | null>(null)
  const historySubjectId = ref<number | null>(null)
  const historyCompanyId = ref<number | null>(null)
  const historyTeacherId = ref<number | null>(null)
  const historyIncludeOpening = ref(false)

  // DataTable global arama kutuları ve panel arama alanları.
  const studentSearch = ref('')
  const companySearch = ref('')
  const companyHoursSearch = ref('')
  const allocationCompanySearch = ref('')

  /**
   * Seçili öğretmen kimliği verilen listede varsa DOKUNMAZ; yoksa (henüz
   * seçim yapılmamış ya da öğretmen listeden düştü/dönem değişti) listenin
   * ilk öğretmenine düşer; liste boşsa `null` olur.
   */
  function syncTeacherSelection(teacherIds: readonly number[]): void {
    const current = selectedTeacherId.value
    if (current !== null && teacherIds.includes(current)) return
    selectedTeacherId.value = teacherIds.length > 0 ? teacherIds[0] : null
  }

  /**
   * Tarihçe filtrelerindeki işletme/öğretmen kimliği güncel seçenek
   * listesinde yoksa temizlenir. `null` zaten "filtre uygulanmıyor" anlamına
   * geldiğinden ona dokunulmaz.
   */
  function syncHistoryOptions(companyIds: readonly number[], teacherIds: readonly number[]): void {
    if (historyCompanyId.value !== null && !companyIds.includes(historyCompanyId.value)) {
      historyCompanyId.value = null
    }
    if (historyTeacherId.value !== null && !teacherIds.includes(historyTeacherId.value)) {
      historyTeacherId.value = null
    }
  }

  /**
   * Tüm seçimleri varsayılana döndürür. Setup store'da otomatik `$reset` yok;
   * bu yüzden her alan burada açıkça sıfırlanır.
   */
  function reset(): void {
    selectedTeacherId.value = null
    historyStream.value = null
    historySubjectId.value = null
    historyCompanyId.value = null
    historyTeacherId.value = null
    historyIncludeOpening.value = false
    studentSearch.value = ''
    companySearch.value = ''
    companyHoursSearch.value = ''
    allocationCompanySearch.value = ''
  }

  return {
    selectedTeacherId,
    historyStream,
    historySubjectId,
    historyCompanyId,
    historyTeacherId,
    historyIncludeOpening,
    studentSearch,
    companySearch,
    companyHoursSearch,
    allocationCompanySearch,
    syncTeacherSelection,
    syncHistoryOptions,
    reset,
  }
})
