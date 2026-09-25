import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import HistoryView from './HistoryView.vue'
import type { Company, HistoryChangeSetEntry, HistoryFilter, HistoryResponse, Teacher } from '../types/models'
import { useSelectionStore } from '../stores/selection'

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
  deleteChangeSet: vi.fn(),
}))

const listCompaniesMock = vi.fn<() => Promise<Company[]>>()
vi.mock('../api/companies', () => ({
  companiesApi: { list: (): Promise<Company[]> => listCompaniesMock() },
}))

const listTeachersMock = vi.fn<() => Promise<Teacher[]>>()
vi.mock('../api/teachers', () => ({
  teachersApi: { list: (): Promise<Teacher[]> => listTeachersMock(), listWithCapacity: vi.fn() },
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
    isDeletable: false,
    warnings: [],
    events: [],
  }
}

function mountView() {
  return mount(HistoryView, {
    global: {
      plugins: [
        [OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }],
        ToastService,
        ConfirmationService,
      ],
    },
    attachTo: document.body,
  })
}

beforeEach(() => {
  listHistoryMock.mockReset()
  listCompaniesMock.mockReset()
  listCompaniesMock.mockResolvedValue([])
  listTeachersMock.mockReset()
  listTeachersMock.mockResolvedValue([])
  document.body.innerHTML = ''
})

function companyFixture(overrides: Partial<Company> = {}): Company {
  return {
    id: 1,
    name: 'Firma A',
    contactFirstName: '',
    contactLastName: '',
    phone: '',
    email: '',
    addressText: '',
    district: '',
    latitude: null,
    longitude: null,
    geocodeStatus: 'pending',
    oneWayDistanceKm: null,
    notes: '',
    createdAt: '2026-01-01',
    updatedAt: '2026-01-01',
    ...overrides,
  }
}

function teacherFixture(overrides: Partial<Teacher> = {}): Teacher {
  return {
    id: 1,
    firstName: 'Ahmet',
    lastName: 'Yılmaz',
    registryNo: '1',
    field: 'Elektrik-Elektronik',
    branches: '[]',
    employmentType: 'tenured',
    baseHours: 0,
    maxExtraHours: 0,
    otherExtraHours: 0,
    chiefType: 'none',
    isActive: 1,
    ...overrides,
  }
}

describe('HistoryView seçim kalıcılığı (Pinia store)', () => {
  it('store’da filtre varken mount edilince listHistory o filtreyle çağrılır', async () => {
    // Arrange
    listHistoryMock.mockResolvedValue({ entries: [], nextBeforeChangeSetId: null })
    listCompaniesMock.mockResolvedValue([companyFixture({ id: 1 })])
    listTeachersMock.mockResolvedValue([teacherFixture({ id: 3 })])
    const selection = useSelectionStore()
    selection.historyStream = 'coordination'
    selection.historyCompanyId = 1
    selection.historyTeacherId = 3
    selection.historyIncludeOpening = true

    // Act
    const wrapper = mountView()
    await flushPromises()

    // Assert
    expect(listHistoryMock.mock.calls[0][0]).toMatchObject({
      stream: 'coordination',
      companyId: 1,
      teacherId: 3,
      includeOpening: true,
    })
    wrapper.unmount()
  })

  it('seçenek listesinde olmayan historyCompanyId mount sonrası temizlenir', async () => {
    // Arrange: store'daki işletme kimliği artık listede yok (silinmiş/bayat).
    listHistoryMock.mockResolvedValue({ entries: [], nextBeforeChangeSetId: null })
    listCompaniesMock.mockResolvedValue([companyFixture({ id: 1 })])
    listTeachersMock.mockResolvedValue([])
    const selection = useSelectionStore()
    selection.historyCompanyId = 99

    // Act
    const wrapper = mountView()
    await flushPromises()

    // Assert
    expect(selection.historyCompanyId).toBeNull()
    wrapper.unmount()
  })
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
