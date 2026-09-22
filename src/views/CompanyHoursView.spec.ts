import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { DOMWrapper, VueWrapper } from '@vue/test-utils'
import { ref } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import CompanyHoursView from './CompanyHoursView.vue'
import { labels } from '../i18n/labels'
import type { AutoDistributeRow, DistributionOutcome, HoursBoard, HoursRow } from '../api/hours'

// Aktif dönem `watch()` ile izlendiği için gerçek bir `ref` olmalı.
vi.mock('../composables/useTerm', () => ({
  activeTerm: ref('2026-2027/1'),
}))

const getBoardMock = vi.fn<() => Promise<HoursBoard>>()
const autoDistributeMock = vi.fn<(rows: AutoDistributeRow[]) => Promise<DistributionOutcome>>()
vi.mock('../api/hours', () => ({
  hoursApi: {
    get: () => getBoardMock(),
    save: vi.fn(),
    autoDistribute: (rows: AutoDistributeRow[]) => autoDistributeMock(rows),
  },
}))

function hoursRow(overrides: Partial<HoursRow> = {}): HoursRow {
  return {
    companyId: 1,
    companyName: 'Akdeniz Elektronik',
    addressText: 'Adres',
    oneWayDistanceKm: 10,
    roundTripDistanceKm: 20,
    studentCount: 2,
    maxHours: 8,
    awardedHours: 4,
    isHonorary: false,
    isLocked: false,
    notes: '',
    isSaved: true,
    ...overrides,
  }
}

function boardFixture(rows: HoursRow[]): HoursBoard {
  return {
    term: '2026-2027/1',
    rows,
    poolHours: 100,
    totalAwarded: rows.reduce((sum, row) => sum + row.awardedHours, 0),
    totalMax: rows.reduce((sum, row) => sum + (row.maxHours ?? 0), 0),
    honoraryCount: 0,
    lockedCount: rows.filter((row) => row.isLocked).length,
    withoutRuleCount: 0,
    warnings: [],
  }
}

async function mountView(): Promise<VueWrapper> {
  // `RouterLink` bir yönlendirici gerektirir.
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/teaching-load', component: { template: '<div />' } },
      { path: '/allocation', component: { template: '<div />' } },
    ],
  })
  await router.push('/')
  await router.isReady()

  const wrapper = mount(CompanyHoursView, {
    global: {
      plugins: [
        [OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }],
        ToastService,
        ConfirmationService,
        router,
      ],
      directives: { tooltip: Tooltip },
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

/** Toplu kilit/aç düğmesi — başlıktaki "Otomatik Dağıt"ın solunda. */
function lockAllButton(wrapper: VueWrapper): DOMWrapper<Element> {
  const button = wrapper
    .findAll('button')
    .find((b) => b.text() === labels.hours.lockAll || b.text() === labels.hours.unlockAll)
  expect(button).toBeDefined()
  return button as DOMWrapper<Element>
}

/** Satır bazlı kilit düğmeleri; hepsinde aynı aria-label var. */
function rowLockButtons(wrapper: VueWrapper): DOMWrapper<Element>[] {
  return wrapper.findAll(`button[aria-label="${labels.hours.locked}"]`)
}

function isRowLockButtonLocked(button: DOMWrapper<Element>): boolean {
  return button.get('.p-button-icon').classes().includes('pi-lock')
}

function saveBadge(wrapper: VueWrapper): string | null | undefined {
  const saveButton = wrapper
    .findAllComponents({ name: 'Button' })
    .find((c) => c.props('label') === labels.hours.save)
  return saveButton?.props('badge') as string | null | undefined
}

beforeEach(() => {
  getBoardMock.mockReset()
  autoDistributeMock.mockReset()
  document.body.replaceChildren()
})

describe('CompanyHoursView toplu kilit düğmesi', () => {
  it('düğmeye basınca tüm satırlar kilitlenir', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(
      boardFixture([hoursRow({ companyId: 1 }), hoursRow({ companyId: 2 }), hoursRow({ companyId: 3 })]),
    )
    const wrapper = await mountView()
    expect(rowLockButtons(wrapper).every((b) => !isRowLockButtonLocked(b))).toBe(true)

    // Act
    await lockAllButton(wrapper).trigger('click')

    // Assert
    expect(rowLockButtons(wrapper)).toHaveLength(3)
    expect(rowLockButtons(wrapper).every((b) => isRowLockButtonLocked(b))).toBe(true)
    wrapper.unmount()
  })

  it('hepsi kilitliyken etiket "Kilitleri Aç"a döner ve basınca hepsi açılır', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(
      boardFixture([hoursRow({ companyId: 1 }), hoursRow({ companyId: 2 })]),
    )
    const wrapper = await mountView()
    await lockAllButton(wrapper).trigger('click')
    expect(lockAllButton(wrapper).text()).toBe(labels.hours.unlockAll)

    // Act
    await lockAllButton(wrapper).trigger('click')

    // Assert
    expect(lockAllButton(wrapper).text()).toBe(labels.hours.lockAll)
    expect(rowLockButtons(wrapper).every((b) => !isRowLockButtonLocked(b))).toBe(true)
    wrapper.unmount()
  })

  it('toplu kilit sonrası değişiklik sayısı satır sayısı kadar artar', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(
      boardFixture([hoursRow({ companyId: 1 }), hoursRow({ companyId: 2 }), hoursRow({ companyId: 3 })]),
    )
    const wrapper = await mountView()
    expect(saveBadge(wrapper)).toBeFalsy()

    // Act
    await lockAllButton(wrapper).trigger('click')

    // Assert — üç satırın tümü `isLocked` bakımından değişti, Kaydet rozeti 3 gösterir.
    expect(saveBadge(wrapper)).toBe('3')
    wrapper.unmount()
  })

  it('toplu kilitten sonra, KAYDETMEDEN, alt satırdaki kilitli sayısı ekrandaki durumu yansıtır', async () => {
    // Arrange — sunucudan yalnız 1 satır kilitli gelir (board.lockedCount === 1).
    getBoardMock.mockResolvedValue(
      boardFixture([
        hoursRow({ companyId: 1, isLocked: false }),
        hoursRow({ companyId: 2, isLocked: true }),
        hoursRow({ companyId: 3, isLocked: false }),
      ]),
    )
    const wrapper = await mountView()
    expect(wrapper.text()).toContain(`1 ${labels.hours.lockedNote}`)

    // Act — kaydetmeden toplu kilitle; `board.lockedCount` hâlâ 1'dir.
    await lockAllButton(wrapper).trigger('click')

    // Assert — ekran kaydedilmemiş CANLI durumu göstermeli: 3, 1 değil.
    expect(wrapper.text()).toContain(`3 ${labels.hours.lockedNote}`)
    expect(wrapper.text()).not.toContain(`1 ${labels.hours.lockedNote}`)
    wrapper.unmount()
  })

  it('bir satır kilitliyken autoDistribute çağrısına giden payloadda o satır isLocked: true taşır', async () => {
    // Arrange — YALNIZCA bir satır kilitli; hepsini kilitlemiyoruz çünkü o
    // durumda düğme artık pasif olur (bkz. "hepsi kilitliyken" testi altta).
    getBoardMock.mockResolvedValue(
      boardFixture([hoursRow({ companyId: 1 }), hoursRow({ companyId: 2 })]),
    )
    autoDistributeMock.mockResolvedValue({
      results: [],
      lockedHours: 0,
      distributedHours: 0,
      leftoverHours: 0,
      honoraryCount: 0,
      warnings: [],
    })
    const wrapper = await mountView()
    await rowLockButtons(wrapper)[0].trigger('click')

    // Act
    const autoDistributeButton = wrapper.findAll('button').find((b) => b.text() === labels.hours.autoDistribute)
    await autoDistributeButton!.trigger('click')
    await flushPromises()

    // Assert
    expect(autoDistributeMock).toHaveBeenCalledTimes(1)
    const payload = autoDistributeMock.mock.calls[0][0]
    expect(payload).toHaveLength(2)
    expect(payload[0].isLocked).toBe(true)
    expect(payload[1].isLocked).toBe(false)
    wrapper.unmount()
  })

  it('kilitli satıra arka uçtan farklı bir sonuç gelse bile ekranda o satır değişmez', async () => {
    // Arrange — 1 numaralı işletme kilitli, mevcut değeri 4. Arka uç (varsayımsal
    // olarak) kilitli satır için de farklı bir awardedHours döndürse bile ekran
    // bunu UYGULAMAMALI — bu, kullanıcının bildirdiği "kilitli satır yine de
    // değişiyor" kusuruna karşı ikinci emniyet katı.
    getBoardMock.mockResolvedValue(
      boardFixture([
        hoursRow({ companyId: 1, isLocked: true, awardedHours: 4 }),
        hoursRow({ companyId: 2, isLocked: false, awardedHours: 2 }),
      ]),
    )
    autoDistributeMock.mockResolvedValue({
      results: [
        { companyId: 1, awardedHours: 9, isHonorary: false, wasLocked: true },
        { companyId: 2, awardedHours: 6, isHonorary: false, wasLocked: false },
      ],
      lockedHours: 4,
      distributedHours: 6,
      leftoverHours: 0,
      honoraryCount: 0,
      warnings: [],
    })
    const wrapper = await mountView()

    // Act
    const autoDistributeButton = wrapper.findAll('button').find((b) => b.text() === labels.hours.autoDistribute)
    await autoDistributeButton!.trigger('click')
    await flushPromises()

    // Assert — kilitli satır (1) eski değerinde (4) kaldı, kilitsiz satır (2)
    // arka ucun döndürdüğü değere (6) güncellendi. Sütun sırası satır sırasıyla
    // aynı olduğundan indeksle eşleştirilir.
    const numberInputs = wrapper.findAllComponents({ name: 'InputNumber' })
    expect(numberInputs).toHaveLength(2)
    expect(numberInputs[0].props('modelValue')).toBe(4)
    expect(numberInputs[1].props('modelValue')).toBe(6)
    wrapper.unmount()
  })

  it('hepsi kilitliyken "Otomatik Dağıt" düğmesi pasif olur', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(
      boardFixture([hoursRow({ companyId: 1 }), hoursRow({ companyId: 2 })]),
    )
    const wrapper = await mountView()

    // Act
    await lockAllButton(wrapper).trigger('click')

    // Assert — OpenVue Button `disabled`i düz DOM özniteliği olarak fallthrough
    // eder (bileşen prop'u olarak değil), bu yüzden `attributes()` kullanılır.
    const autoDistributeButton = wrapper.findAll('button').find((b) => b.text() === labels.hours.autoDistribute)
    expect(autoDistributeButton?.attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })

  it('kilitler açılınca "Otomatik Dağıt" düğmesi yeniden etkinleşir', async () => {
    // Arrange
    getBoardMock.mockResolvedValue(
      boardFixture([hoursRow({ companyId: 1 }), hoursRow({ companyId: 2 })]),
    )
    const wrapper = await mountView()
    await lockAllButton(wrapper).trigger('click')

    // Act — "Kilitleri Aç"a basınca hepsi açılır.
    await lockAllButton(wrapper).trigger('click')

    // Assert
    const autoDistributeButton = wrapper.findAll('button').find((b) => b.text() === labels.hours.autoDistribute)
    expect(autoDistributeButton?.attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })
})
