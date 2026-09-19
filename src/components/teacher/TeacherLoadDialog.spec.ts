import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import TeacherLoadDialog from './TeacherLoadDialog.vue'
import EffectiveDateField from '../history/EffectiveDateField.vue'
import type { ChangeOutcome, ChangeRequest, TeacherWithCapacity, TermWithDates } from '../../types/models'

// Backend komutları henüz canlı değil; `api/history.ts` sarmalayıcısı taklit
// edilir (bkz. ImpactDialog.spec.ts, useChange.spec.ts).
const previewChangeMock = vi.fn<(request: ChangeRequest) => Promise<ChangeOutcome>>()
const commitChangeMock = vi.fn<(request: ChangeRequest, expectedHighWater: number | null) => Promise<ChangeOutcome>>()

vi.mock('../../api/history', () => ({
  previewChange: (request: ChangeRequest) => previewChangeMock(request),
  commitChange: (request: ChangeRequest, expectedHighWater: number | null) =>
    commitChangeMock(request, expectedHighWater),
  listHistory: vi.fn(),
  getSubjectHistory: vi.fn(),
}))

const teacherFixture: TeacherWithCapacity = {
  id: 5,
  firstName: 'Ahmet',
  lastName: 'Yılmaz',
  registryNo: '12345',
  field: 'Elektrik-Elektronik Teknolojisi',
  branches: '[]',
  employmentType: 'tenured',
  baseHours: 20,
  maxExtraHours: 24,
  otherExtraHours: 0,
  chiefType: 'none',
  isActive: 1,
  chiefHours: 0,
  statutoryCap: 40,
  capacity: 24,
}

const startedTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-14',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: false,
  defaultAsOf: '2026-10-15',
  earliestAllowedDate: '2026-10-01',
}

function mountDialog(teacher: TeacherWithCapacity | null = teacherFixture) {
  return mount(TeacherLoadDialog, {
    props: { visible: true, teacher, term: startedTerm },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

function getByTestId(testId: string): HTMLElement {
  const element = document.body.querySelector<HTMLElement>(`[data-testid="${testId}"]`)
  if (!element) throw new Error(`data-testid="${testId}" bulunamadı`)
  return element
}

beforeEach(() => {
  previewChangeMock.mockReset()
  commitChangeMock.mockReset()
  document.body.innerHTML = ''
})

describe('TeacherLoadDialog', () => {
  it('builds setTeacherLoad with the effective date', async () => {
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

    const wrapper = mountDialog()
    await nextTick()

    // Dialog içeriği `document.body`'ye teleport edilir (bkz. ImpactDialog.spec.ts).
    const textarea = document.body.querySelector('textarea')
    if (!textarea) throw new Error('textarea bulunamadı')
    textarea.value = 'şeflik güncellemesi'
    textarea.dispatchEvent(new Event('input'))
    await wrapper.findComponent(EffectiveDateField).vm.$emit('update:modelValue', '2026-10-15')
    await nextTick()

    const saveButton = getByTestId('teacher-load-save-button') as HTMLButtonElement
    expect(saveButton.disabled).toBe(false)

    saveButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    const request = previewChangeMock.mock.calls[0][0]
    expect(request.term).toBe('2026-2027/1')
    expect(request.effectiveDate).toBe('2026-10-15')
    expect(request.reason).toBe('şeflik güncellemesi')
    expect(request.command).toEqual({
      type: 'setTeacherLoad',
      teacherId: 5,
      load: {
        baseHours: 20,
        maxExtraHours: 24,
        otherExtraHours: 0,
        chiefType: 'none',
        employmentType: 'tenured',
      },
    })

    wrapper.unmount()
  })

  it('requires a reason', async () => {
    const wrapper = mountDialog()
    await nextTick()

    await wrapper.findComponent(EffectiveDateField).vm.$emit('update:modelValue', '2026-10-15')
    await nextTick()

    const saveButton = getByTestId('teacher-load-save-button') as HTMLButtonElement
    expect(saveButton.disabled).toBe(true)

    saveButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()
    expect(previewChangeMock).not.toHaveBeenCalled()

    wrapper.unmount()
  })
})
