import { defineStore } from 'pinia'
import { ref } from 'vue'

/** Kullanıcının tema tercihi. 'system' işletim sistemini izler. */
export type ThemePreference = 'light' | 'dark' | 'system'

const STORAGE_KEY = 'mesnet-lite-theme'

/**
 * OpenVue yapılandırmasındaki `darkModeSelector` ile birebir aynı olmalıdır.
 * Değişirse main.ts içindeki ayar da değişmelidir.
 */
const DARK_CLASS = 'app-dark'

function prefersDarkSystem(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

function resolveIsDark(value: ThemePreference): boolean {
  if (value === 'system') return prefersDarkSystem()
  return value === 'dark'
}

function applyToDocument(dark: boolean): void {
  document.documentElement.classList.toggle(DARK_CLASS, dark)
}

function readStoredPreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored === 'light' || stored === 'dark' || stored === 'system') return stored
  } catch {
    // Depolama kapalıysa tercih oturumluk kalır; bu bir hata değil.
  }
  return 'system'
}

function storePreference(value: ThemePreference): void {
  try {
    localStorage.setItem(STORAGE_KEY, value)
  } catch {
    // Yazılamazsa sessizce geç: tema yine de bu oturum için uygulanır.
  }
}

/**
 * Uygulamanın tema tercihi. Diğer paylaşılan durumun aksine tema tercihi
 * `localStorage`'da KALICI tutulur — "yalnız bellek" kuralının bilinen
 * istisnası (mevcut davranış): tema, uygulama kapanıp açıldığında korunur.
 */
export const useThemeStore = defineStore('theme', () => {
  const preference = ref<ThemePreference>('system')
  const isDark = ref(false)

  function setThemePreference(value: ThemePreference): void {
    preference.value = value
    isDark.value = resolveIsDark(value)
    applyToDocument(isDark.value)
    storePreference(value)
  }

  /** Uygulama açılışında bir kez çağrılır. */
  function init(): void {
    setThemePreference(readStoredPreference())

    // Tercih 'system' ise işletim sistemi değiştikçe tema da değişmeli.
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (preference.value === 'system') setThemePreference('system')
    })
  }

  return { preference, isDark, setThemePreference, init }
})
