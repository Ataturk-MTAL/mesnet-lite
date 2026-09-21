import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import AllocationView from './AllocationView.vue'
import type { AssignmentBoard, BoardCompany } from '../api/assignments'
import { labels } from '../i18n/labels'

// Dönem, `AllocationView` içinde `watch()` ile izlenir; gerçek bir `ref` olmalı.
vi.mock('../composables/useTerm', () => ({
  activeTerm: ref('2026-2027/1'),
}))

const getBoardMock = vi.fn<() => Promise<AssignmentBoard>>()
vi.mock('../api/assignments', async () => {
  const actual = await vi.importActual<typeof import('../api/assignments')>('../api/assignments')
  return {
    ...actual,
    assignmentsApi: {
      get: () => getBoardMock(),
      propose: vi.fn(),
      assign: vi.fn(),
      unassign: vi.fn(),
      clear: vi.fn(),
    },
  }
})

function companyFixture(overrides: Partial<BoardCompany> = {}): BoardCompany {
  return {
    companyId: 1,
    companyName: 'Firma A',
    addressText: 'Adres',
    district: 'Akdeniz',
    oneWayDistanceKm: null,
    studentCount: 1,
    studentNames: [],
    branches: [],
    awardedHours: 4,
    isHonorary: false,
    hoursMissing: false,
    workplaceDays: [1],
    assignedTeacherId: null,
    visitDay: null,
    visitHour: null,
    visitEndHour: null,
    isForced: false,
    forceReason: null,
    ...overrides,
  }
}

function boardFixture(companies: BoardCompany[]): AssignmentBoard {
  return {
    term: '2026-2027/1',
    teachers: [],
    companies,
    dayStartHour: 8,
    dayEndHour: 16,
    poolHours: 100,
    assignedHours: 0,
    remainingHours: 100,
    assignedCompanyCount: 0,
    totalCompanyCount: companies.length,
    honoraryCount: 0,
    warnings: [],
  }
}

async function mountView(companies: BoardCompany[]) {
  getBoardMock.mockResolvedValue(boardFixture(companies))
  const wrapper = mount(AllocationView, {
    global: {
      plugins: [
        [OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }],
        ToastService,
        ConfirmationService,
      ],
      directives: { tooltip: Tooltip },
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  getBoardMock.mockReset()
  document.body.replaceChildren()
})

describe('AllocationView atanmamış işletme gruplaması', () => {
  it('işletmeleri ilçeye göre gruplar ve her başlıkta ilçe adı ile sayıyı gösterir', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' }),
      companyFixture({ companyId: 2, companyName: 'Firma B', district: 'Akdeniz' }),
      companyFixture({ companyId: 3, companyName: 'Firma C', district: 'Toroslar' }),
    ])

    // Assert
    const text = wrapper.text()
    expect(text).toContain('Akdeniz (2)')
    expect(text).toContain('Toroslar (1)')
    wrapper.unmount()
  })

  it('grupları Türkçe alfabetik sıraya göre gösterir', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Yenişehir' }),
      companyFixture({ companyId: 2, companyName: 'Firma B', district: 'Akdeniz' }),
      companyFixture({ companyId: 3, companyName: 'Firma C', district: 'Mezitli' }),
    ])

    // Assert
    const headers = wrapper.findAll('.p-panel-title').map((el) => el.text())
    expect(headers).toEqual(['Akdeniz (1)', 'Mezitli (1)', 'Yenişehir (1)'])
    wrapper.unmount()
  })

  it('ilçesi boş olan işletmeler için sonda "İlçe belirsiz" grubu oluşturur', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' }),
      companyFixture({ companyId: 2, companyName: 'Firma B', district: '' }),
    ])

    // Assert
    const headers = wrapper.findAll('.p-panel-title').map((el) => el.text())
    expect(headers).toEqual(['Akdeniz (1)', `${labels.allocation.districtUnknown} (1)`])
    wrapper.unmount()
  })

  it('tüm işletmelerin ilçesi doluysa "İlçe belirsiz" grubu hiç gösterilmez', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' }),
    ])

    // Assert
    expect(wrapper.text()).not.toContain(labels.allocation.districtUnknown)
    wrapper.unmount()
  })

  it('arama sonucu boş kalan grubu gizler, eşleşen grubu ve sayacı korur', async () => {
    // Arrange
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma Akdeniz A', district: 'Akdeniz' }),
      companyFixture({ companyId: 2, companyName: 'Firma Akdeniz B', district: 'Akdeniz' }),
      companyFixture({ companyId: 3, companyName: 'Firma Toroslar', district: 'Toroslar' }),
    ])

    // Act
    await wrapper.get('input[type="text"]').setValue('Toroslar')
    await flushPromises()

    // Assert: yalnızca eşleşen grup görünür, ilçe sayacı ilçedeki TOPLAM işletmeyi
    // (aramadan bağımsız) yansıtır.
    const headers = wrapper.findAll('.p-panel-title').map((el) => el.text())
    expect(headers).toEqual(['Toroslar (1)'])
    expect(wrapper.text()).not.toContain('Akdeniz')
    wrapper.unmount()
  })

  it('sürükleme için kartlar grup içinde hâlâ draggable kalır', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' }),
    ])

    // Assert
    const card = wrapper.find('.company-card')
    expect(card.attributes('draggable')).toBe('true')
    wrapper.unmount()
  })
})

describe('AllocationView işletme adresi', () => {
  it('adresi olan bir atanmamış işletmenin kartında adres metni çizilir', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({
        companyId: 1,
        companyName: 'Firma A',
        district: 'Akdeniz',
        addressText: 'Karaduvar Mah. Serbest Bölge 14.Cadde No:13, Akdeniz/Mersin',
      }),
    ])

    // Assert
    const address = wrapper.find('.company-address')
    expect(address.exists()).toBe(true)
    expect(address.text()).toBe('Karaduvar Mah. Serbest Bölge 14.Cadde No:13, Akdeniz/Mersin')
    wrapper.unmount()
  })

  it('adresi boş olan bir işletmenin kartında adres satırı hiç render edilmez', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz', addressText: '' }),
    ])

    // Assert
    expect(wrapper.find('.company-address').exists()).toBe(false)
    wrapper.unmount()
  })

  it('atanmış işletme kartında da adres görünür', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({
        companyId: 1,
        companyName: 'Firma A',
        district: 'Akdeniz',
        addressText: 'Hürriyet Mah. Hüseyin Okan Merzeci Blv No:489, Yenişehir/Mersin',
        assignedTeacherId: 7,
      }),
    ])

    // Assert
    const addresses = wrapper.findAll('.company-address')
    expect(addresses).toHaveLength(1)
    expect(addresses[0]?.text()).toBe('Hürriyet Mah. Hüseyin Okan Merzeci Blv No:489, Yenişehir/Mersin')
    wrapper.unmount()
  })
})
