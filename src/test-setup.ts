import { beforeEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

// Store artık bileşen ömründen uzun yaşıyor; her testten önce YENİ bir Pinia
// etkinleştirilir ki testler arasında seçim durumu sızmasın. Bileşen
// `mount()`'ları `global.plugins`'e Pinia eklemese de burada etkinleştirilen
// örneğe düşer (Pinia, enjeksiyon bağlamında bulamazsa `activePinia`'ya
// geri döner).
beforeEach(() => {
  setActivePinia(createPinia())
})

// jsdom `matchMedia` sağlamaz; tema composable'ı sistem tercihini onunla okur.
// Varsayılan olarak açık tema döndürülür.
if (typeof window.matchMedia !== 'function') {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  })
}

// Leaflet haritası kapsayıcı boyutunu ResizeObserver ile izler.
if (typeof globalThis.ResizeObserver !== 'function') {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  } as unknown as typeof ResizeObserver
}
