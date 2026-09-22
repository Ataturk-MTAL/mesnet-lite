import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import InputNumber from 'openvue/inputnumber'
import SettingsView from './SettingsView.vue'
import { labels } from '../i18n/labels'
import type { SettingsMap } from '../api/settings'
import type { User } from '../types/models'

const getMock = vi.fn<() => Promise<SettingsMap>>()
const saveMock = vi.fn<(entries: SettingsMap) => Promise<SettingsMap>>()
vi.mock('../api/settings', () => ({
  settingsApi: {
    get: () => getMock(),
    save: (entries: SettingsMap) => saveMock(entries),
  },
}))

// PIN'li kullanıcılar; Ayarlar ekranındaki "Kullanıcılar" bölümü bunları listeler.
const listUsersMock = vi.fn<() => Promise<User[]>>()
const createUserMock = vi.fn<(name: string, pin: string) => Promise<User>>()
const renameUserMock = vi.fn<(id: number, name: string) => Promise<void>>()
const setUserPinMock = vi.fn<(id: number, pin: string) => Promise<void>>()
const setUserActiveMock = vi.fn<(id: number, isActive: boolean) => Promise<void>>()
vi.mock('../api/users', () => ({
  usersApi: {
    hasAny: vi.fn(),
    list: () => listUsersMock(),
    create: (name: string, pin: string) => createUserMock(name, pin),
    rename: (id: number, name: string) => renameUserMock(id, name),
    setPin: (id: number, pin: string) => setUserPinMock(id, pin),
    setActive: (id: number, isActive: boolean) => setUserActiveMock(id, isActive),
    login: vi.fn(),
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

/** Teleport edilen diyalog alanları `wrapper.find()` ile bulunamaz; ham DOM üzerinden yazılır. */
function setNativeValue(input: HTMLInputElement, value: string): void {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

beforeEach(() => {
  getMock.mockReset()
  saveMock.mockReset()
  listUsersMock.mockReset()
  createUserMock.mockReset()
  renameUserMock.mockReset()
  setUserPinMock.mockReset()
  setUserActiveMock.mockReset()
  listUsersMock.mockResolvedValue([])
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

/** Ders saati sayısı `InputNumber`'ı; `input-id="max-daily-lessons"` ile bulunur. */
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

  it('saves only max_daily_lessons, without day_start_hour or day_end_hour', async () => {
    getMock.mockResolvedValue({ ...baseSettings, max_daily_lessons: '9' })
    saveMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await lessonsField(wrapper).vm.$emit('update:modelValue', 7)
    await clickSave(wrapper)
    await vi.waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1))

    const entries = saveMock.mock.calls[0][0]
    expect(entries.max_daily_lessons).toBe('7')
    expect(entries.day_start_hour).toBeUndefined()
    expect(entries.day_end_hour).toBeUndefined()
    wrapper.unmount()
  })

  it('keeps writing only max_daily_lessons for an unchanged legacy record', async () => {
    getMock.mockResolvedValue(baseSettings)
    saveMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await clickSave(wrapper)
    await vi.waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1))

    const entries = saveMock.mock.calls[0][0]
    expect(entries.max_daily_lessons).toBe('9')
    expect(entries.day_start_hour).toBeUndefined()
    expect(entries.day_end_hour).toBeUndefined()
    wrapper.unmount()
  })

  it('does not save when the count exceeds the upper bound of 23', async () => {
    getMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await lessonsField(wrapper).vm.$emit('update:modelValue', 24) // üst sınır 23
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

  it('accepts the exact upper bound of 23', async () => {
    getMock.mockResolvedValue(baseSettings)
    saveMock.mockResolvedValue(baseSettings)

    const wrapper = mountView()
    await flushPromises()
    await lessonsField(wrapper).vm.$emit('update:modelValue', 23)
    await clickSave(wrapper)
    await vi.waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1))

    expect(saveMock.mock.calls[0][0].max_daily_lessons).toBe('23')
    wrapper.unmount()
  })
})

describe('SettingsView users section', () => {
  const sampleUsers: User[] = [
    { id: 1, name: 'Hakan GÜLEN', isActive: true },
    { id: 2, name: 'Ayşe Kaya', isActive: true },
    { id: 3, name: 'Eski Kullanıcı', isActive: false },
  ]

  it('renders the user table including an inactive user, tagged as such', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)

    const wrapper = mountView()
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('Hakan GÜLEN')
    expect(text).toContain('Ayşe Kaya')
    // Pasif kullanıcı listeden kaybolmaz; etiketiyle görünür.
    expect(text).toContain('Eski Kullanıcı')
    expect(text).toContain(labels.settings.users.inactive)

    wrapper.unmount()
  })

  it('creates a user through the create dialog', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    createUserMock.mockResolvedValue({ id: 4, name: 'Yeni Kullanıcı', isActive: true })

    const wrapper = mountView()
    await flushPromises()

    const addButton = wrapper.findAll('button').find((b) => b.text() === labels.settings.users.add)
    await addButton!.trigger('click')
    await flushPromises()

    // Dialog `appendTo="body"` ile document.body'ye teleport edilir; wrapper.find()
    // kendi kök altındaki DOM'u arar, teleport edilen içeriği bulamaz.
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-name')!, 'Yeni Kullanıcı')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-pin')!, '1234')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-pin-confirm')!, '1234')
    await flushPromises()

    const saveButton = document.body.querySelector<HTMLButtonElement>(
      '[data-testid="user-create-save-button"]',
    )!
    saveButton.click()
    await flushPromises()

    expect(createUserMock).toHaveBeenCalledWith('Yeni Kullanıcı', '1234')
    wrapper.unmount()
  })

  it('toggles a user active/inactive and reloads the list', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    setUserActiveMock.mockResolvedValue(undefined)

    const wrapper = mountView()
    await flushPromises()

    const toggleButtons = wrapper.findAll(`button[aria-label="${labels.settings.users.deactivate}"]`)
    await toggleButtons[0].trigger('click')
    await flushPromises()

    expect(setUserActiveMock).toHaveBeenCalledWith(1, false)
    expect(listUsersMock).toHaveBeenCalledTimes(2)
    wrapper.unmount()
  })
})
