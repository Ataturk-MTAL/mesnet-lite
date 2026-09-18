import { call } from './client'

/** Ayarlar anahtar/değer olarak saklanır; tipli okuma yardımcıları aşağıda. */
export type SettingsMap = Record<string, string>

export const settingsApi = {
  get: (): Promise<SettingsMap> => call('get_settings'),
  save: (entries: SettingsMap): Promise<SettingsMap> => call('save_settings', { entries }),
  setSchoolLocation: (latitude: number, longitude: number): Promise<void> =>
    call('set_school_location', { latitude, longitude }),
}
