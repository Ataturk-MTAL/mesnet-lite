import { defineStore } from 'pinia'
import { ref } from 'vue'
import { settingsApi } from '../api/settings'
import { listTermsWithDates, termsApi } from '../api/terms'
import type { TermWithDates } from '../types/models'

/**
 * Aktif eğitim-öğretim yılı.
 *
 * Öğrenciler, atamalar, müsaitlikler ve sınıf günleri döneme bağlıdır;
 * işletmeler ise kalıcıdır ve yıllar arasında yeniden kullanılır.
 *
 * Kaynak doğruluk veritabanındaki `settings.active_term` ayarıdır; burada
 * yalnızca önbelleklenir ve değişince ekranlar yeniden yüklensin diye
 * reaktif tutulur.
 */
function mergeActiveTerm(list: string[], active: string): string[] {
  // Aktif dönem henüz hiç öğrencisi olmayan yeni bir yıl olabilir; listede
  // görünmezse seçici onu gösteremez.
  const merged = active.length > 0 ? [...new Set([active, ...list])] : [...list]
  return merged.sort((a, b) => b.localeCompare(a, 'tr'))
}

export const useTermStore = defineStore('term', () => {
  const activeTerm = ref('')
  /** Veritabanındaki dönemler + aktif dönem, en yeniden eskiye. */
  const terms = ref<string[]>([])
  /**
   * Aktif dönemin başlangıç/bitiş tarihi ve planlama durumu. Henüz
   * yüklenmediyse ya da aktif dönem `list_terms_with_dates`'te yoksa `null`.
   * Saat Ayarları ve Dağıtım ekranları, dönem başladıysa (`isPlanning ===
   * false`) yazımdan önce yürürlük tarihi ve gerekçe sormak için bunu okur.
   */
  const activeTermDates = ref<TermWithDates | null>(null)

  /** `activeTermDates`'i aktif döneme göre yeniden yükler. */
  async function fetchActiveTermDates(): Promise<void> {
    const all = await listTermsWithDates()
    activeTermDates.value = all.find((t) => t.term === activeTerm.value) ?? null
  }

  async function loadTerms(): Promise<void> {
    const settings = await settingsApi.get()
    const active = settings.active_term ?? ''
    // `get_known_terms` döneme bağlı HER tablonun birleşimidir; eski
    // `studentsApi.listTerms()` yalnızca öğrencisi olan dönemleri döndürdüğü
    // için henüz öğrencisi girilmemiş yeni bir eğitim-öğretim yılını gizliyordu.
    const list = await termsApi.list()

    activeTerm.value = active
    terms.value = mergeActiveTerm(list, active)
    await fetchActiveTermDates()
  }

  /** Aktif dönemi değiştirir ve ayara yazar. */
  async function setActiveTerm(term: string): Promise<void> {
    if (term === activeTerm.value) return

    const settings = await settingsApi.get()
    await settingsApi.save({ ...settings, active_term: term })

    activeTerm.value = term
    terms.value = mergeActiveTerm(terms.value, term)
    await fetchActiveTermDates()
  }

  return { activeTerm, terms, activeTermDates, loadTerms, setActiveTerm }
})
