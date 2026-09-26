import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import AvailabilityView from './AvailabilityView.vue'
import EffectiveDateField from '../components/history/EffectiveDateField.vue'
import ChangeDetailsDialog from '../components/history/ChangeDetailsDialog.vue'
import type { AvailabilityBoard, CopyOutcome, SlotInput } from '../api/availability'
import { listHistory } from '../api/history'
import { labels } from '../i18n/labels'
import { useSelectionStore } from '../stores/selection'
import { useTermStore } from '../stores/term'
import { useAsOfDateStore } from '../stores/asOfDate'
import type { ChangeOutcome, ChangeRequest, HistoryChangeSetEntry, ImpactSummary, TermWithDates } from '../types/models'

// Dönem, aktif dönem ve müsaitlik verisi Tauri komutlarından gelir; testte backend yoktur.
const listTermsWithDatesMock = vi.fn<() => Promise<TermWithDates[]>>()
vi.mock('../api/terms', () => ({
  listTermsWithDates: () => listTermsWithDatesMock(),
}))

const getBoardMock = vi.fn<(asOf: string | null) => Promise<AvailabilityBoard>>()
const saveTeacherMock = vi.fn<(teacherId: number, slots: SlotInput[]) => Promise<AvailabilityBoard>>()
const saveClassDaysMock = vi.fn<(grade: string, days: number[]) => Promise<AvailabilityBoard>>()
const copyFromTermMock = vi.fn<(fromTerm: string) => Promise<CopyOutcome>>()
vi.mock('../api/availability', () => ({
  availabilityApi: {
    get: (asOf: string | null = null) => getBoardMock(asOf),
    saveTeacher: (teacherId: number, slots: SlotInput[]) => saveTeacherMock(teacherId, slots),
    saveClassDays: (grade: string, days: number[]) => saveClassDaysMock(grade, days),
    copyFromTerm: (fromTerm: string) => copyFromTermMock(fromTerm),
  },
}))

// Backend komutları henüz canlı değil; `api/history.ts` sarmalayıcısı taklit edilir.
const previewChangeMock = vi.fn<(request: ChangeRequest) => Promise<ChangeOutcome>>()
const commitChangeMock = vi.fn<(request: ChangeRequest, expectedHighWater: number | null) => Promise<ChangeOutcome>>()
vi.mock('../api/history', () => ({
  previewChange: (request: ChangeRequest) => previewChangeMock(request),
  commitChange: (request: ChangeRequest, expectedHighWater: number | null) =>
    commitChangeMock(request, expectedHighWater),
  listHistory: vi.fn(),
  getSubjectHistory: vi.fn(),
  deleteChangeSet: vi.fn(),
}))

function boardFixture(): AvailabilityBoard {
  return {
    term: '2026-2027/1',
    teachers: [{ teacherId: 1, teacherName: 'Ahmet Yılmaz', freeSlots: [], freeCount: 0 }],
    classes: [],
    dayStartHour: 8,
    dayEndHour: 10,
    otherTerms: [],
    warnings: [],
  }
}

const planningTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-11-15',
  endDate: '2027-01-31',
  datesConfirmed: false,
  isPlanning: true,
  defaultAsOf: '2026-11-15',
  earliestAllowedDate: '2026-11-15',
}

const startedTerm: TermWithDates = {
  ...planningTerm,
  startDate: '2026-09-14',
  isPlanning: false,
  earliestAllowedDate: '2026-10-01',
}

function mountView() {
  return mount(AvailabilityView, {
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

type MountedView = ReturnType<typeof mountView>

/** "Boş Saatleri Kaydet"e basar; tarih ve gerekçe artık sayfada değil, açılan pencerede sorulur. */
async function clickSave(wrapper: MountedView): Promise<void> {
  wrapper
    .find('[data-testid="availability-save-teacher-button"]')
    .element.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await vi.waitFor(() => expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull())
}

function detailsReason(): HTMLTextAreaElement {
  return document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
}

/** Penceredeki tarih ve gerekçeyi doldurur ve Kaydet'e basar. */
async function fillDetailsAndConfirm(wrapper: MountedView, date: string, reason: string): Promise<void> {
  await wrapper.findComponent(ChangeDetailsDialog).findComponent(EffectiveDateField).vm.$emit('update:modelValue', date)
  detailsReason().value = reason
  detailsReason().dispatchEvent(new Event('input', { bubbles: true }))
  await flushPromises()
  document.body
    .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
    .dispatchEvent(new MouseEvent('click', { bubbles: true }))
}

beforeEach(() => {
  listTermsWithDatesMock.mockReset()
  getBoardMock.mockReset()
  saveTeacherMock.mockReset()
  saveClassDaysMock.mockReset()
  copyFromTermMock.mockReset()
  previewChangeMock.mockReset()
  commitChangeMock.mockReset()
  document.body.innerHTML = ''
  useTermStore().activeTerm = '2026-2027/1'
})

describe('AvailabilityView', () => {
  it('uses setTeacherSchedule once the term has started', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    previewChangeMock.mockResolvedValueOnce({
      status: 'preview',
      impact: {
        effectiveDate: '2026-10-15',
        isPlanning: false,
        shadowedUntil: null,
        primary: [],
        automatic: [],
        warnings: [],
        notices: [],
      },
      highWater: 1,
    })

    const wrapper = mountView()
    await flushPromises()

    // İlk hücreye Enter ile basmak boş saati işaretler (teacherDirty === true).
    await wrapper.find('.grid-cell').trigger('keydown', { key: 'Enter' })
    await flushPromises()

    // Kaydet düğmesi tarih/gerekçeye bağlı değil; yalnız ızgara değişikliğine bağlı.
    const saveButton = wrapper.find('[data-testid="availability-save-teacher-button"]')
      .element as HTMLButtonElement
    expect(saveButton.disabled).toBe(false)

    // Tarih ve gerekçe girilene kadar hiçbir istek gitmez.
    await clickSave(wrapper)
    expect(previewChangeMock).not.toHaveBeenCalled()
    await fillDetailsAndConfirm(wrapper, '2026-10-15', 'program güncellemesi')
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    expect(saveTeacherMock).not.toHaveBeenCalled()
    const request = previewChangeMock.mock.calls[0][0]
    expect(request.term).toBe('2026-2027/1')
    expect(request.effectiveDate).toBe('2026-10-15')
    expect(request.reason).toBe('program güncellemesi')
    expect(request.command.type).toBe('setTeacherSchedule')
    if (request.command.type === 'setTeacherSchedule') {
      expect(request.command.teacherId).toBe(1)
      expect(request.command.slots.length).toBeGreaterThan(0)
    }

    wrapper.unmount()
  })

  it('keeps the direct save while planning', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([planningTerm])
    saveTeacherMock.mockResolvedValue(boardFixture())

    const wrapper = mountView()
    await flushPromises()

    // Planlamada EffectiveDateField hiç görünmez.
    expect(wrapper.findComponent(EffectiveDateField).exists()).toBe(false)

    await wrapper.find('.grid-cell').trigger('keydown', { key: 'Enter' })
    await flushPromises()

    const saveButton = wrapper.find('[data-testid="availability-save-teacher-button"]')
      .element as HTMLButtonElement
    expect(saveButton.disabled).toBe(false)

    saveButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(saveTeacherMock).toHaveBeenCalledTimes(1))

    expect(previewChangeMock).not.toHaveBeenCalled()
    expect(saveTeacherMock).toHaveBeenCalledWith(1, expect.any(Array))

    wrapper.unmount()
  })
})

describe('AvailabilityView seçim kalıcılığı (Pinia store)', () => {
  it('Dağıtım ekranıyla paylaşılan öğretmen seçimi store’dan gelir', async () => {
    // Arrange: store'da öğretmen 2 önceden seçili — Dağıtım ekranında seçilmiş gibi.
    const selection = useSelectionStore()
    selection.selectedTeacherId = 2
    getBoardMock.mockResolvedValue({
      ...boardFixture(),
      teachers: [
        { teacherId: 1, teacherName: 'Ahmet Yılmaz', freeSlots: [], freeCount: 0 },
        { teacherId: 2, teacherName: 'Quinn Harlow', freeSlots: [], freeCount: 0 },
      ],
    })
    listTermsWithDatesMock.mockResolvedValue([planningTerm])

    // Act
    const wrapper = mountView()
    await flushPromises()

    // Assert: seçim değişmedi (2 kaldı) ve seçici bu öğretmenin adını gösteriyor.
    expect(selection.selectedTeacherId).toBe(2)
    expect(wrapper.find('.teacher-select').text()).toContain('Quinn Harlow')
    wrapper.unmount()
  })
})

// --- Geçmiş paneli, düzeltme modu ve kayıttan sonra ızgara ---

const emptyImpact: ImpactSummary = {
  effectiveDate: '2026-10-15',
  isPlanning: false,
  shadowedUntil: null,
  primary: [],
  automatic: [],
  warnings: [],
  notices: [],
}

function lastChange(overrides: Partial<HistoryChangeSetEntry> = {}): HistoryChangeSetEntry {
  return {
    changeSetId: 5,
    recordedAt: '2026-10-01T09:00:00.000Z',
    kind: 'set_teacher_schedule',
    reason: 'eski gerekçe',
    actor: 'okul.mudur.yrd',
    effectiveDate: '2026-10-12',
    documentDate: null,
    revokedByChangeSetId: null,
    revokesChangeSetId: null,
    isRevocable: true,
    isDeletable: false,
    warnings: [],
    events: [],
    ...overrides,
  }
}

function freeCellCount(wrapper: ReturnType<typeof mountView>): number {
  return wrapper.findAll('.grid-cell--free').length
}

describe('AvailabilityView history and correction', () => {
  beforeEach(() => {
    vi.mocked(listHistory).mockReset()
  })

  it('keeps the server state on the grid after a committed change', async () => {
    const saved = { ...boardFixture(), teachers: [{ teacherId: 1, teacherName: 'Ahmet Yılmaz', freeSlots: ['1-8', '1-9'], freeCount: 2 }] }
    getBoardMock.mockResolvedValueOnce(boardFixture()).mockResolvedValue(saved)
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    vi.mocked(listHistory).mockResolvedValue({ entries: [], nextBeforeChangeSetId: null })
    previewChangeMock.mockResolvedValue({ status: 'preview', impact: emptyImpact, highWater: 3 })
    commitChangeMock.mockResolvedValue({ status: 'committed', changeSetId: 6, impact: emptyImpact })

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('.grid-cell').trigger('keydown', { key: 'Enter' })
    await flushPromises()

    await clickSave(wrapper)
    await fillDetailsAndConfirm(wrapper, '2026-10-15', 'program güncellemesi')
    await vi.waitFor(() => expect(document.body.querySelector('[data-testid="impact-confirm-button"]')).not.toBeNull())
    document.body
      .querySelector<HTMLButtonElement>('[data-testid="impact-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(commitChangeMock).toHaveBeenCalledTimes(1))
    await vi.waitFor(() => expect(getBoardMock).toHaveBeenCalledTimes(2))
    await flushPromises()

    // Izgara temizlenmez: sunucunun son durumu (iki boş saat) görünür.
    expect(freeCellCount(wrapper)).toBe(2)
    // Form bir sonraki değişikliğe hazırdır: düzeltme modu kapalı, pencere kapalı.
    expect(wrapper.find('[data-testid="availability-correcting-banner"]').exists()).toBe(false)
    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).toBeNull()
    wrapper.unmount()
  })

  it('shows the history panel only once the term has started', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([planningTerm])

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('[data-testid="teacher-schedule-history"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('corrects the last change with the stored date and reason', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    vi.mocked(listHistory).mockResolvedValue({ entries: [lastChange()], nextBeforeChangeSetId: null })
    previewChangeMock.mockResolvedValue({ status: 'preview', impact: emptyImpact, highWater: 4 })

    const wrapper = mountView()
    await flushPromises()
    expect(wrapper.find('[data-testid="availability-correcting-banner"]').exists()).toBe(false)

    await wrapper.get('[data-testid="schedule-history-edit-button"]').trigger('click')
    await flushPromises()

    expect(wrapper.get('[data-testid="availability-correcting-banner"]').text()).toContain(labels.availability.correcting)

    // Izgara değişmese bile tarih/gerekçe düzeltmesi kaydedilebilir.
    const save = wrapper.find('[data-testid="availability-save-teacher-button"]').element as HTMLButtonElement
    expect(save.disabled).toBe(false)

    // Pencere düzeltilen değişikliğin tarih ve gerekçesiyle dolu açılır; değerleri değiştirmeden onaylanır.
    await clickSave(wrapper)
    expect(wrapper.findComponent(ChangeDetailsDialog).findComponent(EffectiveDateField).props('modelValue')).toBe('2026-10-12')
    expect(detailsReason().value).toBe('eski gerekçe')
    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    const request = previewChangeMock.mock.calls[0][0]
    expect(request.effectiveDate).toBe('2026-10-12')
    expect(request.reason).toBe('eski gerekçe')
    expect(request.command).toEqual({
      type: 'correct',
      changeSetId: 5,
      replacement: { type: 'setTeacherSchedule', teacherId: 1, slots: [] },
    })
    expect(saveTeacherMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('leaves correction mode on cancel and sends a plain schedule again', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    vi.mocked(listHistory).mockResolvedValue({ entries: [lastChange()], nextBeforeChangeSetId: null })
    previewChangeMock.mockResolvedValue({ status: 'preview', impact: emptyImpact, highWater: 4 })

    const wrapper = mountView()
    await flushPromises()
    await wrapper.get('[data-testid="schedule-history-edit-button"]').trigger('click')
    await wrapper.get('[data-testid="availability-cancel-correction-button"]').trigger('click')
    await flushPromises()

    expect(wrapper.find('[data-testid="availability-correcting-banner"]').exists()).toBe(false)

    await wrapper.find('.grid-cell').trigger('keydown', { key: 'Enter' })
    await flushPromises()
    await clickSave(wrapper)
    // Düzeltme bittiği için pencere boş açılır; eski değerler taşınmaz.
    expect(detailsReason().value).toBe('')
    await fillDetailsAndConfirm(wrapper, '2026-10-15', 'yeni değişiklik')
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    expect(previewChangeMock.mock.calls[0][0].command.type).toBe('setTeacherSchedule')
    wrapper.unmount()
  })

  it('keeps the grid draft when the details dialog is cancelled', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    vi.mocked(listHistory).mockResolvedValue({ entries: [], nextBeforeChangeSetId: null })

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('.grid-cell').trigger('keydown', { key: 'Enter' })
    expect(freeCellCount(wrapper)).toBe(1)

    await clickSave(wrapper)
    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-cancel-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(previewChangeMock).not.toHaveBeenCalled()
    expect(freeCellCount(wrapper)).toBe(1)
    wrapper.unmount()
  })

  it('does not show any required-field warning on the page itself', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    vi.mocked(listHistory).mockResolvedValue({ entries: [], nextBeforeChangeSetId: null })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('textarea#availability-reason').exists()).toBe(false)
    expect(wrapper.text()).not.toContain(labels.history.reasonRequired)
    expect(wrapper.text()).not.toContain(labels.effectiveDateField.required)
    wrapper.unmount()
  })
})

describe('AvailabilityView tarihteki durum (asOf)', () => {
  it('varsayılan tarihte board `null` ile istenir, başlık yoktur ve ızgara düzenlenebilir', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    vi.mocked(listHistory).mockResolvedValue({ entries: [], nextBeforeChangeSetId: null })
    useTermStore().activeTermDates = startedTerm

    const wrapper = mountView()
    await flushPromises()

    expect(getBoardMock).toHaveBeenCalledWith(null)
    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(false)
    await wrapper.find('.grid-cell').trigger('keydown', { key: 'Enter' })
    expect(freeCellCount(wrapper)).toBe(1)
    wrapper.unmount()
  })

  it('geçmiş bir tarih seçilince board o tarihle istenir, başlık görünür ve ızgara salt okunur olur', async () => {
    getBoardMock.mockResolvedValue(boardFixture())
    listTermsWithDatesMock.mockResolvedValue([startedTerm])
    vi.mocked(listHistory).mockResolvedValue({ entries: [], nextBeforeChangeSetId: null })
    useTermStore().activeTermDates = startedTerm
    useAsOfDateStore().setAsOfDate('2026-10-05')

    const wrapper = mountView()
    await flushPromises()

    expect(getBoardMock).toHaveBeenCalledWith('2026-10-05')
    expect(wrapper.find('[data-testid="as-of-readonly-banner"]').exists()).toBe(true)

    await wrapper.find('.grid-cell').trigger('keydown', { key: 'Enter' })
    expect(freeCellCount(wrapper)).toBe(0)

    const selectAllButton = wrapper.findAll('button').find((b) => b.text() === labels.availability.selectAll)
    expect(selectAllButton?.attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })
})
