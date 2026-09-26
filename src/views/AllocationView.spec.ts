import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { DOMWrapper, VueWrapper } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import AllocationView from './AllocationView.vue'
import ChangeDetailsDialog from '../components/history/ChangeDetailsDialog.vue'
import EffectiveDateField from '../components/history/EffectiveDateField.vue'
import type {
  AllocationProposal,
  AssignmentBoard,
  BoardCompany,
  BoardTeacher,
  NewAssignment,
} from '../api/assignments'
import { labels } from '../i18n/labels'
import { useSelectionStore } from '../stores/selection'
import { useTermStore } from '../stores/term'
import { useAsOfDateStore } from '../stores/asOfDate'
import type { EffectiveChangeInput, TermWithDates } from '../types/models'

const getBoardMock = vi.fn<(asOf: string | null) => Promise<AssignmentBoard>>()
const proposeMock = vi.fn<() => Promise<AllocationProposal>>()
const assignMock = vi.fn<(input: NewAssignment, change?: EffectiveChangeInput) => Promise<AssignmentBoard>>()
const unassignMock = vi.fn<(companyId: number, change?: EffectiveChangeInput) => Promise<AssignmentBoard>>()
const clearMock = vi.fn<(change?: EffectiveChangeInput) => Promise<AssignmentBoard>>()
vi.mock('../api/assignments', async () => {
  const actual = await vi.importActual<typeof import('../api/assignments')>('../api/assignments')
  return {
    ...actual,
    assignmentsApi: {
      get: (asOf: string | null = null) => getBoardMock(asOf),
      propose: () => proposeMock(),
      assign: (input: NewAssignment, change?: EffectiveChangeInput) => assignMock(input, change),
      unassign: (companyId: number, change?: EffectiveChangeInput) => unassignMock(companyId, change),
      clear: (change?: EffectiveChangeInput) => clearMock(change),
    },
  }
})

// AllocationView `<Toast />`'u kendi içinde barındırmaz (App.vue'da yaşar); çakışma
// engelinde gösterilen hata mesajını DOM yerine bu casusla doğrularız.
const toastAddMock = vi.fn<(message: { severity: string; summary?: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

/** Planlama evresindeki bir dönem — eski testler tarih penceresi görmeden geçer. */
function planningTermDates(overrides: Partial<TermWithDates> = {}): TermWithDates {
  return {
    term: '2026-2027/1',
    startDate: '2026-09-01',
    endDate: '2027-06-30',
    datesConfirmed: true,
    isPlanning: true,
    defaultAsOf: '2026-09-26',
    earliestAllowedDate: '2026-09-01',
    ...overrides,
  }
}

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
  proposeMock.mockReset()
  assignMock.mockReset()
  unassignMock.mockReset()
  clearMock.mockReset()
  toastAddMock.mockReset()
  document.body.replaceChildren()
  const termStore = useTermStore()
  termStore.activeTerm = '2026-2027/1'
  // Varsayılan: planlama evresi — mevcut testler tarih penceresi görmeden geçer.
  termStore.activeTermDates = planningTermDates()
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

/** Izgaradaki `(day, hour)` hücresi; satırlar saate, sütunlar `DAYS = [1..5]`e göre sıralı. */
function gridCellAt(wrapper: VueWrapper, dayStartHour: number, day: number, hour: number): DOMWrapper<Element> {
  const flatIndex = (hour - dayStartHour) * 5 + (day - 1)
  const cell = wrapper.findAll('.grid-cell')[flatIndex]
  expect(cell).toBeDefined()
  return cell as DOMWrapper<Element>
}

function changeDetailsReasonInput(): HTMLTextAreaElement {
  return document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
}

async function fillChangeDetailsAndConfirm(wrapper: VueWrapper, date: string, reason: string): Promise<void> {
  await wrapper.findComponent(ChangeDetailsDialog).findComponent(EffectiveDateField).vm.$emit('update:modelValue', date)
  changeDetailsReasonInput().value = reason
  changeDetailsReasonInput().dispatchEvent(new Event('input', { bubbles: true }))
  await flushPromises()
  document.body
    .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
    .dispatchEvent(new MouseEvent('click', { bubbles: true }))
}

/** Klavyeyle kart seçip hedef hücreye tıklayarak yerleştirme yapar (sürükle-bırak jsdom'da güvenilir değil). */
async function selectAndPlace(
  wrapper: VueWrapper,
  companyCardIndex: number,
  dayStartHour: number,
  day: number,
  hour: number,
): Promise<void> {
  await wrapper.findAll('.company-card')[companyCardIndex].trigger('keydown', { key: 'Enter' })
  await gridCellAt(wrapper, dayStartHour, day, hour).trigger('click')
}

describe('AllocationView dönem başladıysa yazımlar tarih penceresinden geçer', () => {
  function startedTeacher(): BoardTeacher {
    return teacherFixture({
      teacherId: 1,
      freeSlots: ['1-9', '2-9'],
      occupiedBy: {},
      hoursPerDay: {},
      assignedHours: 0,
    })
  }

  beforeEach(() => {
    useTermStore().activeTermDates = planningTermDates({ isPlanning: false })
  })

  it('yerleştirme önce pencere açar; onayda assign tarih ve gerekçeyle çağrılır', async () => {
    // Arrange
    const company = companyFixture({ companyId: 1, companyName: 'Firma A', workplaceDays: [1], awardedHours: 1 })
    const teacher = startedTeacher()
    const wrapper = await mountView([company], { teachers: [teacher], dayStartHour: 8, dayEndHour: 16 })
    useSelectionStore().selectedTeacherId = teacher.teacherId
    await flushPromises()
    assignMock.mockResolvedValue(boardFixture([company], { teachers: [teacher] }))

    // Act — kartı seç, boş hücreye tıkla.
    await selectAndPlace(wrapper, 0, 8, 1, 9)
    await flushPromises()

    // Assert — pencere açıldı, henüz yazılmadı.
    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull()
    expect(assignMock).not.toHaveBeenCalled()

    // Act
    await fillChangeDetailsAndConfirm(wrapper, '2026-10-10', 'Ekim yerleşimi')
    await flushPromises()

    // Assert
    expect(assignMock).toHaveBeenCalledTimes(1)
    expect(assignMock.mock.calls[0][0]).toEqual({
      teacherId: teacher.teacherId,
      companyId: 1,
      visitDay: 1,
      visitHour: 9,
      isForced: false,
      forceReason: null,
    })
    expect(assignMock.mock.calls[0][1]).toEqual({ effectiveDate: '2026-10-10', reason: 'Ekim yerleşimi' })
    wrapper.unmount()
  })

  it('çıkarma önce pencere açar; onayda unassign tarih ve gerekçeyle çağrılır', async () => {
    // Arrange — işletme zaten atanmış.
    const company = companyFixture({
      companyId: 1,
      companyName: 'Firma A',
      assignedTeacherId: 1,
      visitDay: 1,
      visitHour: 9,
      visitEndHour: 9,
    })
    const teacher = startedTeacher()
    const wrapper = await mountView([company], { teachers: [teacher], assignedCompanyCount: 1 })
    unassignMock.mockResolvedValue(boardFixture([company], { teachers: [teacher] }))

    // Act — atanmış işletmeler listesindeki "x" düğmesi.
    const removeButton = wrapper.findAll(`button[aria-label="${labels.allocation.removeAssignment}"]`)[0]
    await removeButton!.trigger('click')
    await flushPromises()

    // Assert
    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull()
    expect(unassignMock).not.toHaveBeenCalled()

    // Act
    await fillChangeDetailsAndConfirm(wrapper, '2026-10-11', 'Çıkarma gerekçesi')
    await flushPromises()

    // Assert
    expect(unassignMock).toHaveBeenCalledWith(1, { effectiveDate: '2026-10-11', reason: 'Çıkarma gerekçesi' })
    wrapper.unmount()
  })

  it('öneri uygulama TÜM atamalar için TEK pencere açar, aynı tarihi her kaleme yollar', async () => {
    // Arrange
    const teacher = startedTeacher()
    const companyA = companyFixture({ companyId: 1, companyName: 'Firma A' })
    const companyB = companyFixture({ companyId: 2, companyName: 'Firma B' })
    const wrapper = await mountView([companyA, companyB], { teachers: [teacher] })
    proposeMock.mockResolvedValue({
      assignments: [
        {
          companyId: 1,
          companyName: 'Firma A',
          teacherId: 1,
          teacherName: teacher.teacherName,
          awardedHours: 1,
          visitDay: 1,
          visitHour: 9,
          exactBranchMatch: true,
        },
        {
          companyId: 2,
          companyName: 'Firma B',
          teacherId: 1,
          teacherName: teacher.teacherName,
          awardedHours: 1,
          visitDay: 2,
          visitHour: 9,
          exactBranchMatch: true,
        },
      ],
      unassigned: [],
    })
    assignMock.mockResolvedValue(boardFixture([companyA, companyB], { teachers: [teacher] }))

    // Act — öneriyi üret, uygula. Öneri diyaloğu `body`e teleport edildiğinden
    // "Uygula" düğmesi `wrapper` yerine `document.body` üzerinden aranır.
    const proposeButton = wrapper.findAll('button').find((b) => b.text().includes(labels.allocation.propose))
    await proposeButton!.trigger('click')
    await flushPromises()
    const applyButton = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes(labels.allocation.proposalApply),
    )
    expect(applyButton).toBeDefined()
    applyButton!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    // Assert — TEK pencere açıldı, henüz hiçbir atama yazılmadı.
    expect(document.body.querySelectorAll('[data-testid="change-details-dialog"]')).toHaveLength(1)
    expect(assignMock).not.toHaveBeenCalled()

    // Act
    await fillChangeDetailsAndConfirm(wrapper, '2026-10-12', 'Öneri uygulaması')
    await flushPromises()

    // Assert — her iki atama da AYNI tarih ve gerekçeyle gitti.
    expect(assignMock).toHaveBeenCalledTimes(2)
    expect(assignMock.mock.calls[0][1]).toEqual({ effectiveDate: '2026-10-12', reason: 'Öneri uygulaması' })
    expect(assignMock.mock.calls[1][1]).toEqual({ effectiveDate: '2026-10-12', reason: 'Öneri uygulaması' })
    wrapper.unmount()
  })

  it('dönem başladıysa "Tüm Atamaları Sil" devre dışıdır', async () => {
    // Arrange & Act
    const company = companyFixture({ companyId: 1, assignedTeacherId: 1 })
    const wrapper = await mountView([company], { assignedCompanyCount: 1 })

    // Assert
    const clearButton = wrapper.findAll('button').find((b) => b.text().includes(labels.allocation.clearAll))
    expect(clearButton?.attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })

  it('ikinci yerleştirmede pencere son seçilen tarihle ön dolu gelir', async () => {
    // Arrange
    const companyA = companyFixture({ companyId: 1, companyName: 'Firma A', workplaceDays: [1], awardedHours: 1 })
    const companyB = companyFixture({ companyId: 2, companyName: 'Firma B', workplaceDays: [2], awardedHours: 1 })
    const teacher = startedTeacher()
    const wrapper = await mountView([companyA, companyB], { teachers: [teacher], dayStartHour: 8, dayEndHour: 16 })
    useSelectionStore().selectedTeacherId = teacher.teacherId
    await flushPromises()
    const afterFirst = boardFixture([companyA, companyB], { teachers: [teacher] })
    assignMock.mockResolvedValue(afterFirst)

    // Act — ilk yerleştirme, tarih gir ve onayla.
    await selectAndPlace(wrapper, 0, 8, 1, 9)
    await flushPromises()
    await fillChangeDetailsAndConfirm(wrapper, '2026-10-13', 'İlk yerleşim')
    await flushPromises()

    // Act — ikinci yerleştirme; pencere yeniden açılır.
    await selectAndPlace(wrapper, 1, 8, 2, 9)
    await flushPromises()

    // Assert — tarih alanı bir önceki onaylanan tarihle ön dolu; gerekçe boş.
    expect(wrapper.findComponent(ChangeDetailsDialog).findComponent(EffectiveDateField).props('modelValue')).toBe(
      '2026-10-13',
    )
    expect(changeDetailsReasonInput().value).toBe('')
    wrapper.unmount()
  })
})

describe('AllocationView çakışma engellemesi', () => {
  /** Bir slotu (1. gün 9. ders) başka bir işletmenin bloğuyla dolu gösteren öğretmen. */
  function teacherWithOccupiedSlot(): BoardTeacher {
    return teacherFixture({
      teacherId: 1,
      freeSlots: ['1-9'],
      occupiedBy: { '1-9': 2 },
      hoursPerDay: { '1': 1 },
      assignedHours: 1,
    })
  }

  function occupyingCompany(companyName: string): BoardCompany {
    return companyFixture({
      companyId: 2,
      companyName,
      assignedTeacherId: 1,
      visitDay: 1,
      visitHour: 9,
      visitEndHour: 9,
      awardedHours: 1,
    })
  }

  it('çakışan yerleştirmede zorlama penceresi açılmaz, assign çağrılmaz, hata Toast’u gösterilir', async () => {
    // Arrange — hedef işletmenin tek kural dışılığı çakışma olsun diye günü/boş saati temiz.
    const target = companyFixture({ companyId: 1, companyName: 'Firma A', workplaceDays: [1], awardedHours: 1 })
    const occupying = occupyingCompany('Firma B')
    const teacher = teacherWithOccupiedSlot()
    const wrapper = await mountView([target, occupying], { teachers: [teacher], dayStartHour: 8, dayEndHour: 16 })
    useSelectionStore().selectedTeacherId = teacher.teacherId
    await flushPromises()

    // Act
    await selectAndPlace(wrapper, 0, 8, 1, 9)
    await flushPromises()

    // Assert — pencere hiç açılmadı, yazma denenmedi, hata Toast'u işletme adını taşıyor.
    expect(document.body.textContent).not.toContain(labels.allocation.forceQuestion)
    expect(assignMock).not.toHaveBeenCalled()
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: labels.allocation.overlapViolation('Firma B') }),
    )
    wrapper.unmount()
  })

  it('çakışmayan ama kural dışı (boş saat dışı) yerleştirmede zorlama penceresi eskisi gibi açılır', async () => {
    // Arrange — öğretmenin hiç boş saati yok, çakışma yok.
    const target = companyFixture({ companyId: 1, companyName: 'Firma A', workplaceDays: [1], awardedHours: 1 })
    const teacher = teacherFixture({ teacherId: 1, freeSlots: [], occupiedBy: {} })
    const wrapper = await mountView([target], { teachers: [teacher], dayStartHour: 8, dayEndHour: 16 })
    useSelectionStore().selectedTeacherId = teacher.teacherId
    await flushPromises()

    // Act
    await selectAndPlace(wrapper, 0, 8, 1, 9)
    await flushPromises()

    // Assert — pencere açıldı, henüz yazılmadı.
    expect(document.body.textContent).toContain(labels.allocation.forceQuestion)
    expect(assignMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('çakışma başka bir kural dışı durumla birlikteyse de yerleştirme engellenir (çakışma baskın)', async () => {
    // Arrange — hedef, hem işletme günü uymuyor HEM çakışıyor.
    const target = companyFixture({ companyId: 1, companyName: 'Firma A', workplaceDays: [2], awardedHours: 1 })
    const occupying = occupyingCompany('Firma C')
    const teacher = teacherWithOccupiedSlot()
    const wrapper = await mountView([target, occupying], { teachers: [teacher], dayStartHour: 8, dayEndHour: 16 })
    useSelectionStore().selectedTeacherId = teacher.teacherId
    await flushPromises()

    // Act
    await selectAndPlace(wrapper, 0, 8, 1, 9)
    await flushPromises()

    // Assert
    expect(document.body.textContent).not.toContain(labels.allocation.forceQuestion)
    expect(assignMock).not.toHaveBeenCalled()
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: labels.allocation.overlapViolation('Firma C') }),
    )
    wrapper.unmount()
  })
})

describe('AllocationView tarihteki durum (asOf)', () => {
  it('varsayılan tarihte board `null` ile istenir ve yazma düğmeleri açıktır', async () => {
    const wrapper = await mountView([companyFixture({ companyId: 1 })])

    expect(getBoardMock).toHaveBeenCalledWith(null)
    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(false)
    const proposeButton = wrapper.findAll('button').find((b) => b.text().includes(labels.allocation.propose))
    expect(proposeButton?.attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it('geçmiş bir tarih seçilince board o tarihle istenir, başlık görünür ve yazma düğmeleri kapanır', async () => {
    useAsOfDateStore().setAsOfDate('2026-09-10')
    const wrapper = await mountView([companyFixture({ companyId: 1 })])

    expect(getBoardMock).toHaveBeenCalledWith('2026-09-10')
    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(true)

    const proposeButton = wrapper.findAll('button').find((b) => b.text().includes(labels.allocation.propose))
    expect(proposeButton?.attributes('disabled')).toBeDefined()

    const card = wrapper.find('.company-card')
    expect(card.attributes('draggable')).toBe('false')
    wrapper.unmount()
  })
})
