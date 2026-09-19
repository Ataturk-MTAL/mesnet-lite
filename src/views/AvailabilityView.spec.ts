import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import AvailabilityView from './AvailabilityView.vue'
import EffectiveDateField from '../components/history/EffectiveDateField.vue'
import type { AvailabilityBoard, CopyOutcome, SlotInput } from '../api/availability'
import type { ChangeOutcome, ChangeRequest, TermWithDates } from '../types/models'

// Dönem, aktif dönem ve müsaitlik verisi Tauri komutlarından gelir; testte backend yoktur.
// `AvailabilityView` bunu `watch()` ile izlediği için gerçek bir `ref` olmalı
// (AppSidebar.spec.ts'teki düz nesne burada Vue'nun watch uyarısını tetikler).
vi.mock('../composables/useTerm', () => ({
  activeTerm: ref('2026-2027/1'),
}))

const listTermsWithDatesMock = vi.fn<() => Promise<TermWithDates[]>>()
vi.mock('../api/terms', () => ({
  listTermsWithDates: () => listTermsWithDatesMock(),
}))

const getBoardMock = vi.fn<() => Promise<AvailabilityBoard>>()
const saveTeacherMock = vi.fn<(teacherId: number, slots: SlotInput[]) => Promise<AvailabilityBoard>>()
const saveClassDaysMock = vi.fn<(grade: string, days: number[]) => Promise<AvailabilityBoard>>()
const copyFromTermMock = vi.fn<(fromTerm: string) => Promise<CopyOutcome>>()
vi.mock('../api/availability', () => ({
  availabilityApi: {
    get: () => getBoardMock(),
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
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
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
    await wrapper.findComponent(EffectiveDateField).vm.$emit('update:modelValue', '2026-10-15')
    await wrapper.find('textarea#availability-reason').setValue('program güncellemesi')
    await flushPromises()

    const saveButton = wrapper.find('[data-testid="availability-save-teacher-button"]')
      .element as HTMLButtonElement
    expect(saveButton.disabled).toBe(false)

    saveButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
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
