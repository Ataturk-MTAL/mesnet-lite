import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import TeachersView from './TeachersView.vue'
import EffectiveDateField from '../components/history/EffectiveDateField.vue'
import { labels } from '../i18n/labels'
import { useTermStore } from '../stores/term'
import type { TeacherWithCapacity, TermWithDates } from '../types/models'

// Tauri çalışma zamanı testte yoktur; komut adları ve argümanlar burada gözlenir
// (bkz. ImportExportView.spec.ts).
const callMock = vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>()
vi.mock('../api/client', () => ({
  call: (command: string, args?: Record<string, unknown>) => callMock(command, args),
}))

function mountView() {
  return mount(TeachersView, {
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

function mockDefaultCommands(): void {
  callMock.mockImplementation(async (command) => {
    if (command === 'list_teachers_with_capacity') return [teacherFixture]
    if (command === 'list_students') return []
    if (command === 'list_terms_with_dates') return [startedTerm]
    if (command === 'update_teacher') return { ...teacherFixture, employmentType: 'contracted' }
    return undefined
  })
}

beforeEach(() => {
  callMock.mockReset()
  document.body.innerHTML = ''
  // Dönem store'u gerçek Pinia store'dur; API'ye gitmeden doğrudan doldurulur.
  useTermStore().activeTerm = '2026-2027/1'
})

describe('TeachersView — yürürlük tarihi/gerekçe akışı update_teacher üzerinde', () => {
  it('dönem başladıktan sonra istihdam türünü değiştirirken önce tarih/gerekçe sorar, sonra bunları update_teacher isteğine ekler', async () => {
    mockDefaultCommands()
    const wrapper = mountView()

    await vi.waitFor(() => expect(wrapper.text()).toContain('Ahmet'))

    document
      .body!
      .querySelector<HTMLButtonElement>('[aria-label="' + labels.common.edit + '"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    wrapper.findAllComponents({ name: 'Select' })[0].vm.$emit('update:modelValue', 'contracted')
    await flushPromises()

    document.body
      .querySelector<HTMLButtonElement>('[data-testid="teacher-form-save-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    await vi.waitFor(() =>
      expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull(),
    )

    await wrapper.findComponent(EffectiveDateField).vm.$emit('update:modelValue', '2026-10-20')
    const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
    reasonField.value = 'sözleşme türü değişti'
    reasonField.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()

    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() => expect(callMock).toHaveBeenCalledWith('update_teacher', expect.anything()))

    const updateArgs = callMock.mock.calls.find((call) => call[0] === 'update_teacher')![1] as {
      id: number
      effectiveDate: string | null
      reason: string | null
    }
    expect(updateArgs.id).toBe(5)
    expect(updateArgs.effectiveDate).toBe('2026-10-20')
    expect(updateArgs.reason).toBe('sözleşme türü değişti')

    wrapper.unmount()
  })
})
