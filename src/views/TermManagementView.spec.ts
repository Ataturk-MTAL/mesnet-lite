import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import TermManagementView from './TermManagementView.vue'
import TermDatesDialog from '../components/term/TermDatesDialog.vue'
import { labels } from '../i18n/labels'
import type { SettingsMap } from '../api/settings'
import type { TermWithDates } from '../types/models'
import type { CreateTermResult } from '../api/terms'

// Toast bileşeni bu görünümde değil App'te durur; bildirim çağrısı doğrudan gözlenir.
const toastAddMock = vi.fn<(message: { severity: string; summary?: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

// Dönem, aktif dönem ve tarih verisi Tauri komutlarından gelir; testte backend yoktur.
const getSettingsMock = vi.fn<() => Promise<SettingsMap>>()
const saveSettingsMock = vi.fn<(entries: SettingsMap) => Promise<SettingsMap>>()
vi.mock('../api/settings', () => ({
  settingsApi: {
    get: () => getSettingsMock(),
    save: (entries: SettingsMap) => saveSettingsMock(entries),
  },
}))

const listMock = vi.fn<() => Promise<string[]>>()
const createMock = vi.fn<(term: string, copyTeachingLoadFromTerm: string | null) => Promise<CreateTermResult>>()
const listTermsWithDatesMock = vi.fn<() => Promise<TermWithDates[]>>()
const updateTermDatesMock = vi.fn<
  (input: { term: string; startDate: string; endDate: string; confirm: boolean }) => Promise<TermWithDates>
>()
vi.mock('../api/terms', () => ({
  termsApi: {
    list: () => listMock(),
    create: (term: string, copyTeachingLoadFromTerm: string | null) => createMock(term, copyTeachingLoadFromTerm),
  },
  listTermsWithDates: () => listTermsWithDatesMock(),
  updateTermDates: (input: { term: string; startDate: string; endDate: string; confirm: boolean }) =>
    updateTermDatesMock(input),
}))

function termFixture(overrides: Partial<TermWithDates> = {}): TermWithDates {
  return {
    term: '2026-2027/1',
    startDate: '2026-09-01',
    endDate: '2027-01-31',
    datesConfirmed: false,
    isPlanning: true,
    defaultAsOf: '2026-09-26',
    earliestAllowedDate: '2026-09-01',
    ...overrides,
  }
}

async function mountView(): Promise<VueWrapper> {
  const wrapper = mount(TermManagementView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

function editButtonFor(term: string): HTMLButtonElement {
  const button = Array.from(document.body.querySelectorAll<HTMLButtonElement>('[data-testid="term-dates-edit-button"]'))
    .find((b) => b.closest('tr')?.textContent?.includes(term))
  if (!button) throw new Error(`"${term}" için Tarihleri Düzenle düğmesi bulunamadı`)
  return button
}

beforeEach(() => {
  getSettingsMock.mockReset()
  saveSettingsMock.mockReset()
  listMock.mockReset()
  createMock.mockReset()
  listTermsWithDatesMock.mockReset()
  updateTermDatesMock.mockReset()
  toastAddMock.mockReset()
  document.body.innerHTML = ''

  getSettingsMock.mockResolvedValue({ active_term: '2026-2027/1' })
  listMock.mockResolvedValue(['2026-2027/1', '2025-2026/1'])
  listTermsWithDatesMock.mockResolvedValue([
    termFixture({ term: '2026-2027/1', startDate: '2026-09-01', endDate: '2027-01-31', datesConfirmed: false }),
    termFixture({ term: '2025-2026/1', startDate: '2025-09-01', endDate: '2026-06-30', datesConfirmed: true }),
  ])
})

describe('TermManagementView — tablo', () => {
  it('her dönem satırında başlangıç, bitiş ve onay durumunu gösterir', async () => {
    // Arrange & Act
    const wrapper = await mountView()

    // Assert
    const tableText = wrapper.get('table').text()
    expect(tableText).toContain('2026-09-01')
    expect(tableText).toContain('2027-01-31')
    expect(tableText).toContain('2025-09-01')
    expect(tableText).toContain('2026-06-30')
    expect(tableText).toContain(labels.termManagement.datesUnconfirmed)
    expect(tableText).toContain(labels.termManagement.datesConfirmed)
    wrapper.unmount()
  })
})

describe('TermManagementView — düzenleme penceresi', () => {
  it('"Tarihleri Düzenle" ilgili satırın mevcut değerleriyle pencereyi açar', async () => {
    // Arrange
    const wrapper = await mountView()

    // Act
    editButtonFor('2025-2026/1').dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    // Assert
    const dialog = wrapper.findComponent(TermDatesDialog)
    expect(dialog.props('visible')).toBe(true)
    expect(dialog.props('term')).toEqual(
      termFixture({ term: '2025-2026/1', startDate: '2025-09-01', endDate: '2026-06-30', datesConfirmed: true }),
    )
    wrapper.unmount()
  })

  it('Kaydet, updateTermDates`i doğru payload ile çağırır ve tabloyla dönem deposunu yeniler', async () => {
    // Arrange
    updateTermDatesMock.mockResolvedValue(
      termFixture({ term: '2026-2027/1', startDate: '2026-09-15', endDate: '2027-06-30', datesConfirmed: true }),
    )
    const wrapper = await mountView()
    editButtonFor('2026-2027/1').dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    listTermsWithDatesMock.mockResolvedValue([
      termFixture({ term: '2026-2027/1', startDate: '2026-09-15', endDate: '2027-06-30', datesConfirmed: true }),
      termFixture({ term: '2025-2026/1', startDate: '2025-09-01', endDate: '2026-06-30', datesConfirmed: true }),
    ])

    // Act
    await wrapper
      .findComponent(TermDatesDialog)
      .vm.$emit('save', { startDate: '2026-09-15', endDate: '2027-06-30', confirmed: true })
    await flushPromises()

    // Assert
    expect(updateTermDatesMock).toHaveBeenCalledWith({
      term: '2026-2027/1',
      startDate: '2026-09-15',
      endDate: '2027-06-30',
      confirm: true,
    })
    // İkinci `listTermsWithDates` çağrısı `load()`'dan, üçüncüsü `termStore.loadTerms()`'ten gelir.
    expect(listTermsWithDatesMock.mock.calls.length).toBeGreaterThanOrEqual(3)
    expect(wrapper.findComponent(TermDatesDialog).props('visible')).toBe(false)
    wrapper.unmount()
  })

  it('arka uç reddederse Türkçe mesajı gösterir ve pencere açık kalır', async () => {
    // Arrange
    updateTermDatesMock.mockRejectedValue(new Error('update_term_dates: Bitiş tarihi başlangıç tarihinden önce olamaz'))
    const wrapper = await mountView()
    editButtonFor('2026-2027/1').dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    // Act
    await wrapper
      .findComponent(TermDatesDialog)
      .vm.$emit('save', { startDate: '2027-01-01', endDate: '2026-01-01', confirmed: false })
    await flushPromises()

    // Assert — Rust mesajı olduğu gibi Toast'a gider, pencere kapanmaz.
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: 'error',
        detail: expect.stringContaining('Bitiş tarihi başlangıç tarihinden önce olamaz'),
      }),
    )
    expect(wrapper.findComponent(TermDatesDialog).props('visible')).toBe(true)
    wrapper.unmount()
  })
})

describe('TermManagementView — onaysız dönem uyarısı', () => {
  it('aktif dönemin tarihleri onaylanmadıysa uyarı başlığı görünür ve düğme o dönemin penceresini açar', async () => {
    // Arrange & Act
    const wrapper = await mountView()

    // Assert
    const banner = wrapper.get('[data-testid="term-dates-unconfirmed-banner"]')
    expect(banner.text()).toContain(labels.termManagement.datesUnconfirmedWarning)

    // Act — banner düğmesi aktif dönemin (2026-2027/1) penceresini açsın.
    await wrapper.get('[data-testid="term-dates-warning-edit-button"]').trigger('click')

    // Assert
    const dialog = wrapper.findComponent(TermDatesDialog)
    expect(dialog.props('visible')).toBe(true)
    expect(dialog.props('term')?.term).toBe('2026-2027/1')
    wrapper.unmount()
  })

  it('aktif dönemin tarihleri onaylıysa uyarı başlığı görünmez', async () => {
    // Arrange
    getSettingsMock.mockResolvedValue({ active_term: '2025-2026/1' })

    // Act
    const wrapper = await mountView()

    // Assert
    expect(wrapper.find('[data-testid="term-dates-unconfirmed-banner"]').exists()).toBe(false)
    wrapper.unmount()
  })
})
