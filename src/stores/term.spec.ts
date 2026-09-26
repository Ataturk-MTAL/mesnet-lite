import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useTermStore } from './term'
import type { SettingsMap } from '../api/settings'
import type { TermWithDates } from '../types/models'

const getMock = vi.fn<() => Promise<SettingsMap>>()
const saveMock = vi.fn<(entries: SettingsMap) => Promise<SettingsMap>>()
vi.mock('../api/settings', () => ({
  settingsApi: {
    get: () => getMock(),
    save: (entries: SettingsMap) => saveMock(entries),
  },
}))

const listMock = vi.fn<() => Promise<string[]>>()
const listWithDatesMock = vi.fn<() => Promise<TermWithDates[]>>()
vi.mock('../api/terms', () => ({
  termsApi: {
    list: () => listMock(),
  },
  listTermsWithDates: () => listWithDatesMock(),
}))

function termWithDates(overrides: Partial<TermWithDates> = {}): TermWithDates {
  return {
    term: '2026-2027/1',
    startDate: '2026-09-01',
    endDate: '2027-06-30',
    datesConfirmed: true,
    isPlanning: false,
    defaultAsOf: '2026-09-26',
    earliestAllowedDate: '2026-09-01',
    ...overrides,
  }
}

beforeEach(() => {
  getMock.mockReset()
  saveMock.mockReset()
  listMock.mockReset()
  listWithDatesMock.mockReset()
  // Varsayılan: boş liste — testler yalnız `activeTermDates`i önemsediğinde kendi verisini verir.
  listWithDatesMock.mockResolvedValue([])
})

describe('useTermStore', () => {
  it('loadTerms aktif dönemi listeye katıp yeniden eskiye sıralar', async () => {
    getMock.mockResolvedValue({ active_term: '2026-2027/1' })
    listMock.mockResolvedValue(['2024-2025/1', '2025-2026/1'])

    const store = useTermStore()
    await store.loadTerms()

    expect(store.activeTerm).toBe('2026-2027/1')
    expect(store.terms).toEqual(['2026-2027/1', '2025-2026/1', '2024-2025/1'])
  })

  it('setActiveTerm ayara yazar ve listeye ekler', async () => {
    getMock.mockResolvedValue({ active_term: '2025-2026/1' })
    saveMock.mockResolvedValue({ active_term: '2026-2027/1' })
    const store = useTermStore()
    store.terms = ['2025-2026/1']
    store.activeTerm = '2025-2026/1'

    await store.setActiveTerm('2026-2027/1')

    expect(saveMock).toHaveBeenCalledWith({ active_term: '2026-2027/1' })
    expect(store.activeTerm).toBe('2026-2027/1')
    expect(store.terms).toEqual(['2026-2027/1', '2025-2026/1'])
  })

  it('aynı dönemde setActiveTerm API çağrısı yapmaz', async () => {
    const store = useTermStore()
    store.activeTerm = '2026-2027/1'

    await store.setActiveTerm('2026-2027/1')

    expect(getMock).not.toHaveBeenCalled()
    expect(saveMock).not.toHaveBeenCalled()
  })

  it('loadTerms sonrası activeTermDates aktif dönemin tarihleriyle dolar', async () => {
    getMock.mockResolvedValue({ active_term: '2026-2027/1' })
    listMock.mockResolvedValue(['2026-2027/1'])
    listWithDatesMock.mockResolvedValue([termWithDates({ term: '2026-2027/1', isPlanning: false })])

    const store = useTermStore()
    await store.loadTerms()

    expect(store.activeTermDates).toEqual(termWithDates({ term: '2026-2027/1', isPlanning: false }))
  })

  it('aktif dönem list_terms_with_dates sonucunda yoksa activeTermDates null kalır', async () => {
    getMock.mockResolvedValue({ active_term: '2027-2028/1' })
    listMock.mockResolvedValue(['2027-2028/1'])
    listWithDatesMock.mockResolvedValue([termWithDates({ term: '2026-2027/1' })])

    const store = useTermStore()
    await store.loadTerms()

    expect(store.activeTermDates).toBeNull()
  })

  it('setActiveTerm sonrası activeTermDates yeni aktif döneme göre güncellenir', async () => {
    getMock.mockResolvedValue({ active_term: '2025-2026/1' })
    listWithDatesMock.mockResolvedValue([
      termWithDates({ term: '2025-2026/1', isPlanning: false }),
      termWithDates({ term: '2026-2027/1', isPlanning: true }),
    ])
    const store = useTermStore()
    store.terms = ['2025-2026/1']
    store.activeTerm = '2025-2026/1'

    await store.setActiveTerm('2026-2027/1')

    expect(store.activeTermDates).toEqual(termWithDates({ term: '2026-2027/1', isPlanning: true }))
  })
})
