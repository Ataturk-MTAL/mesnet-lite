import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { useThemeStore } from './theme'

const STORAGE_KEY = 'mesnet-lite-theme'
const DARK_CLASS = 'app-dark'

beforeEach(() => {
  localStorage.clear()
  document.documentElement.classList.remove(DARK_CLASS)
})

afterEach(() => {
  localStorage.clear()
  document.documentElement.classList.remove(DARK_CLASS)
})

describe('useThemeStore', () => {
  it('kayıtlı tercih okunur', () => {
    localStorage.setItem(STORAGE_KEY, 'dark')

    const store = useThemeStore()
    store.init()

    expect(store.preference).toBe('dark')
    expect(store.isDark).toBe(true)
  })

  it('geçersiz değer system e düşer', () => {
    localStorage.setItem(STORAGE_KEY, 'not-a-theme')

    const store = useThemeStore()
    store.init()

    expect(store.preference).toBe('system')
  })

  it('setThemePreference app-dark sınıfını uygular ve localStorage a yazar', () => {
    const store = useThemeStore()

    store.setThemePreference('dark')

    expect(document.documentElement.classList.contains(DARK_CLASS)).toBe(true)
    expect(localStorage.getItem(STORAGE_KEY)).toBe('dark')
  })

  it('setThemePreference light seçilince app-dark sınıfını kaldırır', () => {
    const store = useThemeStore()
    store.setThemePreference('dark')

    store.setThemePreference('light')

    expect(document.documentElement.classList.contains(DARK_CLASS)).toBe(false)
    expect(localStorage.getItem(STORAGE_KEY)).toBe('light')
  })
})
