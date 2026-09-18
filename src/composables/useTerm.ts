import { ref, readonly } from 'vue'
import { settingsApi } from '../api/settings'
import { studentsApi } from '../api/students'

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
const activeTermRef = ref('')
const availableTerms = ref<string[]>([])
const isLoaded = ref(false)

export const activeTerm = readonly(activeTermRef)

/** Veritabanındaki dönemler + aktif dönem, en yeniden eskiye. */
export const terms = readonly(availableTerms)

function mergeActiveTerm(list: string[], active: string): string[] {
  // Aktif dönem henüz hiç öğrencisi olmayan yeni bir yıl olabilir; listede
  // görünmezse seçici onu gösteremez.
  const merged = active.length > 0 ? [...new Set([active, ...list])] : [...list]
  return merged.sort((a, b) => b.localeCompare(a, 'tr'))
}

export async function loadTerms(): Promise<void> {
  const settings = await settingsApi.get()
  const active = settings.active_term ?? ''
  const list = await studentsApi.listTerms()

  activeTermRef.value = active
  availableTerms.value = mergeActiveTerm(list, active)
  isLoaded.value = true
}

/** Aktif dönemi değiştirir ve ayara yazar. */
export async function setActiveTerm(term: string): Promise<void> {
  if (term === activeTermRef.value) return

  const settings = await settingsApi.get()
  await settingsApi.save({ ...settings, active_term: term })

  activeTermRef.value = term
  availableTerms.value = mergeActiveTerm(availableTerms.value, term)
}

export function useTerm() {
  return { activeTerm, terms, isLoaded: readonly(isLoaded), loadTerms, setActiveTerm }
}
