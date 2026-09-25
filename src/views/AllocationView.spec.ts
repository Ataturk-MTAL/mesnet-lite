import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import AllocationView from './AllocationView.vue'
import type { AssignmentBoard, BoardCompany, BoardTeacher } from '../api/assignments'
import { labels } from '../i18n/labels'
import { useSelectionStore } from '../stores/selection'

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

function teacherFixture(overrides: Partial<BoardTeacher> = {}): BoardTeacher {
  return {
    teacherId: 1,
    teacherName: 'Ahmet Yılmaz',
    branches: ['Elektrik-Elektronik'],
    capacity: 40,
    assignedHours: 0,
    companyCount: 0,
    freeSlots: [],
    occupiedBy: {},
    hoursPerDay: {},
    daysOverCap: [],
    isOverCapacity: false,
    ...overrides,
  }
}

function boardFixture(
  companies: BoardCompany[],
  overrides: Partial<AssignmentBoard> = {},
): AssignmentBoard {
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
    ...overrides,
  }
}

async function mountView(companies: BoardCompany[], boardOverrides: Partial<AssignmentBoard> = {}) {
  getBoardMock.mockResolvedValue(boardFixture(companies, boardOverrides))
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

describe('AllocationView sol panelin kendi içinde kayması', () => {
  it('ilçe gruplarını "company-list" sarmalayıcısının içinde çizer', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' }),
      companyFixture({ companyId: 2, companyName: 'Firma B', district: 'Toroslar' }),
    ])

    // Assert: sayfa değil kartın kendisi kayacağı için ilçe grupları
    // "company-list" sarmalayıcısının içinde olmalı.
    const list = wrapper.find('.company-list')
    expect(list.exists()).toBe(true)
    expect(list.findAll('.district-group')).toHaveLength(2)
    wrapper.unmount()
  })

  it('arama kutusunu "company-list" sarmalayıcısının dışında tutar, liste kayarken sabit kalsın', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' }),
    ])

    // Assert
    const list = wrapper.find('.company-list')
    expect(list.find('input[type="text"]').exists()).toBe(false)
    expect(wrapper.find('.search').exists()).toBe(true)
    wrapper.unmount()
  })

  it('atanmış işletmeler panelini de "company-list" sarmalayıcısının içinde tutar', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz', assignedTeacherId: 7 }),
    ])

    // Assert
    const list = wrapper.find('.company-list')
    expect(list.find('.assigned-panel').exists()).toBe(true)
    wrapper.unmount()
  })

  it('sol kartı "panel-slot" yuvasının içine koyar ki listenin uzunluğu satır yüksekliğini etkilemesin', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' }),
    ])

    // Assert: yuva akıştan çıkarılan kartı taşımalı; satırın (dolayısıyla sağ
    // paneldeki ızgaranın) yüksekliğine sol listenin içeriği katkı vermemeli.
    const slot = wrapper.find('.panel-slot')
    expect(slot.exists()).toBe(true)
    expect(slot.find('.panel--list').exists()).toBe(true)
    wrapper.unmount()
  })
})

describe('AllocationView sağ panelin kendi içinde kayması', () => {
  it('haftalık ızgarayı "grid-scroll" sarmalayıcısının içinde çizer, öğretmen seçici dışında kalır', async () => {
    // Arrange & Act: geniş saat aralığı ızgarayı uzatır, sarmalayıcı bunu kendi
    // içinde kaydırmalı.
    const wrapper = await mountView(
      [companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' })],
      { teachers: [teacherFixture()], dayStartHour: 8, dayEndHour: 22 },
    )

    // Assert
    const gridScroll = wrapper.find('.grid-scroll')
    expect(gridScroll.exists()).toBe(true)
    expect(gridScroll.find('table.grid').exists()).toBe(true)
    expect(gridScroll.find('.teacher-select').exists()).toBe(false)
    expect(wrapper.find('.teacher-select').exists()).toBe(true)
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

describe('AllocationView seçim kalıcılığı (Pinia store)', () => {
  it('seçili öğretmen unmount + yeniden mount edilince korunur', async () => {
    // Arrange
    const teachers = [teacherFixture({ teacherId: 1 }), teacherFixture({ teacherId: 2 })]
    const selection = useSelectionStore()
    const wrapper = await mountView([], { teachers })
    selection.selectedTeacherId = 2
    await flushPromises()
    wrapper.unmount()

    // Act: aynı store'a bağlı ikinci bir mount
    const wrapper2 = await mountView([], { teachers })

    // Assert
    expect(selection.selectedTeacherId).toBe(2)
    wrapper2.unmount()
  })

  it('store bayat (board’da olmayan) bir kimlik taşıyorsa mount edilince ilk öğretmene düşer', async () => {
    // Arrange
    const selection = useSelectionStore()
    selection.selectedTeacherId = 99
    const teachers = [teacherFixture({ teacherId: 1 }), teacherFixture({ teacherId: 2 })]

    // Act
    const wrapper = await mountView([], { teachers })

    // Assert
    expect(selection.selectedTeacherId).toBe(1)
    wrapper.unmount()
  })

  it('allocationCompanySearch yeniden mount edilince korunur', async () => {
    // Arrange
    const company = companyFixture({ companyId: 1, companyName: 'Firma A', district: 'Akdeniz' })
    const wrapper = await mountView([company])
    const selection = useSelectionStore()

    // Act
    await wrapper.get('input[type="text"]').setValue('Toroslar')
    wrapper.unmount()

    // Assert
    expect(selection.allocationCompanySearch).toBe('Toroslar')
    const wrapper2 = await mountView([company])
    expect((wrapper2.get('.search').element as HTMLInputElement).value).toBe('Toroslar')
    wrapper2.unmount()
  })
})
