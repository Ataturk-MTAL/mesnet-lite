import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
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
