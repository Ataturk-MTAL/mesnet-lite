import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import HistoryView from './HistoryView.vue'
import type { Company, HistoryChangeSetEntry, HistoryFilter, HistoryResponse, Teacher } from '../types/models'

// `AvailabilityView.spec.ts`'teki gibi: `watch(activeTerm, load)` çağrıldığı
// için gerçek bir `ref` gerekir, düz nesne Vue'nun watch uyarısını tetikler.
vi.mock('../composables/useTerm', () => ({
  activeTerm: ref('2026-2027/1'),
}))

const listHistoryMock = vi.fn<(filter: HistoryFilter) => Promise<HistoryResponse>>()
vi.mock('../api/history', () => ({
  listHistory: (filter: HistoryFilter) => listHistoryMock(filter),
  previewChange: vi.fn(),
  commitChange: vi.fn(),
  getSubjectHistory: vi.fn(),
}))

vi.mock('../api/companies', () => ({
  companiesApi: { list: (): Promise<Company[]> => Promise.resolve([]) },
}))

vi.mock('../api/teachers', () => ({
  teachersApi: { list: (): Promise<Teacher[]> => Promise.resolve([]), listWithCapacity: vi.fn() },
}))

function entryFixture(changeSetId: number): HistoryChangeSetEntry {
  return {
    changeSetId,
    recordedAt: '2026-09-19T10:00:00.000Z',
    kind: 'transferStudent',
    reason: 'test gerekçesi',
    actor: 'okul.mudur.yrd',
    effectiveDate: '2026-09-20',
    documentDate: null,
    revokedByChangeSetId: null,
    revokesChangeSetId: null,
    isRevocable: true,
    warnings: [],
    events: [],
  }
}

function mountView() {
  return mount(HistoryView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
}

beforeEach(() => {
  listHistoryMock.mockReset()
  document.body.innerHTML = ''
})

describe('HistoryView', () => {
  it('pages with nextBeforeChangeSetId', async () => {
    listHistoryMock
      .mockResolvedValueOnce({ entries: [entryFixture(5), entryFixture(4)], nextBeforeChangeSetId: 4 })
      .mockResolvedValueOnce({ entries: [entryFixture(3)], nextBeforeChangeSetId: null })

    const wrapper = mountView()
    await flushPromises()

    expect(listHistoryMock).toHaveBeenCalledTimes(1)
    expect(listHistoryMock.mock.calls[0][0]).toMatchObject({ beforeChangeSetId: null })
    expect(wrapper.findAll('[data-testid="history-entry-card"]')).toHaveLength(2)

    const loadMoreButton = wrapper.get('[data-testid="history-load-more"]')
    await loadMoreButton.trigger('click')
    await flushPromises()

    expect(listHistoryMock).toHaveBeenCalledTimes(2)
    expect(listHistoryMock.mock.calls[1][0]).toMatchObject({ beforeChangeSetId: 4 })
    expect(wrapper.findAll('[data-testid="history-entry-card"]')).toHaveLength(3)
    expect(wrapper.find('[data-testid="history-load-more"]').exists()).toBe(false)

    wrapper.unmount()
  })

  it('shows the empty state when there are no entries', async () => {
    listHistoryMock.mockResolvedValueOnce({ entries: [], nextBeforeChangeSetId: null })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.findAll('[data-testid="history-entry-card"]')).toHaveLength(0)
    expect(wrapper.find('[data-testid="history-load-more"]').exists()).toBe(false)

    wrapper.unmount()
  })
})
