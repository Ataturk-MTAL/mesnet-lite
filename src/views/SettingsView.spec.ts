import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import InputNumber from 'openvue/inputnumber'
import SettingsView from './SettingsView.vue'
import type { SettingsMap } from '../api/settings'

const getMock = vi.fn<() => Promise<SettingsMap>>()
const saveMock = vi.fn<(entries: SettingsMap) => Promise<SettingsMap>>()
vi.mock('../api/settings', () => ({
  settingsApi: {
    get: () => getMock(),
    save: (entries: SettingsMap) => saveMock(entries),
  },
}))

// Harita Leaflet çizer; bu testin konusu değildir.
const LocationPickerMapStub = { template: '<div />' }

function mountView() {
  return mount(SettingsView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
      stubs: { LocationPickerMap: LocationPickerMapStub },
    },
    attachTo: document.body,
  })
}

const baseSettings: SettingsMap = {
  school_name: 'Atatürk MTAL',
  active_term: '2026-2027/1',
  institution_type: 'other',
  is_metropolitan_district: 'true',
  day_start_hour: '8',
  day_end_hour: '17',
}

beforeEach(() => {
  getMock.mockReset()
  saveMock.mockReset()
  document.body.innerHTML = ''
})

describe('SettingsView principal and field name', () => {
  it('loads principal_name and field_name into the inputs', async () => {
    getMock.mockResolvedValue({ ...baseSettings, principal_name: 'Ali Veli', field_name: 'Elektrik-Elektronik' })

    const wrapper = mountView()
    await flushPromises()

    expect((wrapper.find('#principal-name').element as HTMLInputElement).value).toBe('Ali Veli')
    expect((wrapper.find('#field-name').element as HTMLInputElement).value).toBe('Elektrik-Elektronik')

    wrapper.unmount()
  })

  it('leaves both inputs empty when the settings are absent', async () => {
    getMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()

    expect((wrapper.find('#principal-name').element as HTMLInputElement).value).toBe('')
    expect((wrapper.find('#field-name').element as HTMLInputElement).value).toBe('')

    wrapper.unmount()
  })

  it('saves the edited values under principal_name and field_name', async () => {
    getMock.mockResolvedValue(baseSettings)
    saveMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('#principal-name').setValue('Ayşe Kaya')
    await wrapper.find('#field-name').setValue('Elektrik-Elektronik Teknolojisi')

    const buttons = wrapper.findAll('.actions button')
    await buttons[buttons.length - 1].trigger('click')
    await vi.waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1))

    const entries = saveMock.mock.calls[0][0]
    expect(entries.principal_name).toBe('Ayşe Kaya')
    expect(entries.field_name).toBe('Elektrik-Elektronik Teknolojisi')
    expect(entries.school_name).toBe('Atatürk MTAL')

    wrapper.unmount()
  })
})

function lessonsInput(wrapper: ReturnType<typeof mountView>): HTMLInputElement {
  return wrapper.find('input#max-daily-lessons').element as HTMLInputElement
}

/** Ders saati sayısı `InputNumber`'ı; başlangıç saati alanı `#day-start` kimliğini taşır. */
function lessonsField(wrapper: ReturnType<typeof mountView>) {
  return wrapper.findAllComponents(InputNumber).find((field) => field.props('inputId') === 'max-daily-lessons')!
}

async function clickSave(wrapper: ReturnType<typeof mountView>): Promise<void> {
  const buttons = wrapper.findAll('.actions button')
  await buttons[buttons.length - 1].trigger('click')
  await flushPromises()
}

describe('SettingsView max daily lessons', () => {
  it('loads max_daily_lessons when it exists', async () => {
    getMock.mockResolvedValue({ ...baseSettings, max_daily_lessons: '7' })

    const wrapper = mountView()
    await flushPromises()

    expect(lessonsInput(wrapper).value).toBe('7')
    wrapper.unmount()
  })

  it('derives the count from an old record that only has day_end_hour', async () => {
    // 8 + 9 = 17: eski kurulumda yalnız başlangıç ve bitiş saati vardı.
    getMock.mockResolvedValue({ ...baseSettings, day_start_hour: '9', day_end_hour: '15' })

    const wrapper = mountView()
    await flushPromises()

    expect(lessonsInput(wrapper).value).toBe('6')
    wrapper.unmount()
  })

  it('falls back to 9 when neither key exists', async () => {
    const { day_start_hour: _start, day_end_hour: _end, ...withoutHours } = baseSettings
    getMock.mockResolvedValue(withoutHours)

    const wrapper = mountView()
    await flushPromises()

    expect(lessonsInput(wrapper).value).toBe('9')
    wrapper.unmount()
  })

  it('saves max_daily_lessons and derives day_end_hour from the start hour', async () => {
    getMock.mockResolvedValue({ ...baseSettings, max_daily_lessons: '9' })
    saveMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await lessonsField(wrapper).vm.$emit('update:modelValue', 7)
    await clickSave(wrapper)
    await vi.waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1))

    const entries = saveMock.mock.calls[0][0]
    expect(entries.max_daily_lessons).toBe('7')
    expect(entries.day_start_hour).toBe('8')
    expect(entries.day_end_hour).toBe('15')
    wrapper.unmount()
  })

  it('writes 8 + 9 = 17 for an unchanged legacy record', async () => {
    getMock.mockResolvedValue(baseSettings)
    saveMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await clickSave(wrapper)
    await vi.waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1))

    const entries = saveMock.mock.calls[0][0]
    expect(entries.max_daily_lessons).toBe('9')
    expect(entries.day_end_hour).toBe('17')
    wrapper.unmount()
  })

  it('does not save when start hour plus count exceeds 24', async () => {
    getMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await lessonsField(wrapper).vm.$emit('update:modelValue', 17) // 8 + 17 = 25
    await clickSave(wrapper)

    expect(saveMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('does not save when the count is empty or below 1', async () => {
    getMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await lessonsField(wrapper).vm.$emit('update:modelValue', 0)
    await clickSave(wrapper)
    await lessonsField(wrapper).vm.$emit('update:modelValue', null)
    await clickSave(wrapper)

    expect(saveMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('accepts the exact upper bound (start 8 + 16 = 24)', async () => {
    getMock.mockResolvedValue(baseSettings)
    saveMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await lessonsField(wrapper).vm.$emit('update:modelValue', 16)
    await clickSave(wrapper)
    await vi.waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1))

    expect(saveMock.mock.calls[0][0].day_end_hour).toBe('24')
    wrapper.unmount()
  })

  it('caps the count when the start hour moves later', async () => {
    getMock.mockResolvedValue({ ...baseSettings, max_daily_lessons: '16' })

    const wrapper = mountView()
    await flushPromises()
    const startField = wrapper.findAllComponents(InputNumber).find((field) => field.attributes('id') === 'day-start')!
    await startField.vm.$emit('update:modelValue', 12) // üst sınır 24 - 12 = 12
    await flushPromises()

    expect(lessonsInput(wrapper).value).toBe('12')
    wrapper.unmount()
  })
})
