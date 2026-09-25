import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useTermStore } from './term'
import type { SettingsMap } from '../api/settings'

const getMock = vi.fn<() => Promise<SettingsMap>>()
const saveMock = vi.fn<(entries: SettingsMap) => Promise<SettingsMap>>()
vi.mock('../api/settings', () => ({
  settingsApi: {
    get: () => getMock(),
    save: (entries: SettingsMap) => saveMock(entries),
  },
}))

const listMock = vi.fn<() => Promise<string[]>>()
vi.mock('../api/terms', () => ({
  termsApi: {
    list: () => listMock(),
  },
}))

beforeEach(() => {
  getMock.mockReset()
  saveMock.mockReset()
  listMock.mockReset()
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
})
