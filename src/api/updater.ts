import { call } from './client'

/** Otomatik güncelleme için Tauri komutları. */
export const updaterApi = {
  /**
   * Bu derlemede güncelleme yapılabilir mi? Geliştirme derlemesi,
   * Microsoft Store/MSIX ve .deb/.rpm kurulumlarında `false` döner.
   */
  isSupported: (): Promise<boolean> => call('updater_supported'),
}
