import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import InputNumber from 'openvue/inputnumber'
import SettingsView from './SettingsView.vue'
import { labels } from '../i18n/labels'
import { currentUser, signIn, signOut } from '../composables/useAuth'
import { usersApi } from '../api/users'
import type { SettingsMap } from '../api/settings'
import type { BackupStatus, User } from '../types/models'

const getMock = vi.fn<() => Promise<SettingsMap>>()
const saveMock = vi.fn<(entries: SettingsMap) => Promise<SettingsMap>>()
vi.mock('../api/settings', () => ({
  settingsApi: {
    get: () => getMock(),
    save: (entries: SettingsMap) => saveMock(entries),
  },
}))

// Yedekleme kartı — Tauri komutları ve dosya diyalogları ayrı ayrı casuslanır.
const backupStatusMock = vi.fn<() => Promise<BackupStatus>>()
const createBackupMock = vi.fn<(path: string) => Promise<void>>()
const restoreBackupMock = vi.fn<(path: string) => Promise<void>>()
vi.mock('../api/backup', () => ({
  backupApi: {
    status: () => backupStatusMock(),
    create: (path: string) => createBackupMock(path),
    restore: (path: string) => restoreBackupMock(path),
  },
}))

const saveDialogMock = vi.fn<(options?: unknown) => Promise<string | null>>()
const openDialogMock = vi.fn<(options?: unknown) => Promise<string | string[] | null>>()
vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: (options?: unknown) => saveDialogMock(options),
  open: (options?: unknown) => openDialogMock(options),
}))

const openPathMock = vi.fn<(path: string) => Promise<void>>()
vi.mock('@tauri-apps/plugin-opener', () => ({
  openPath: (path: string) => openPathMock(path),
}))

// `<Toast />` ve `<ConfirmDialog />` App.vue'da yaşar; bu ekran yalnız
// `useToast`/`useConfirm` çağırır. Gerçek diyaloğu çizmek yerine `require`e
// verilen `accept` geri çağrısını yakalayıp elle tetikleriz (bkz. CompaniesView.spec.ts).
const toastAddMock = vi.fn<(message: { severity: string; summary?: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

const confirmRequireMock = vi.fn<(options: { accept?: () => void }) => void>()
vi.mock('openvue/useconfirm', () => ({
  useConfirm: () => ({ require: confirmRequireMock, close: vi.fn() }),
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
      plugins: [
        [OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }],
        ToastService,
        ConfirmationService,
      ],
      stubs: { LocationPickerMap: LocationPickerMapStub },
    },
    attachTo: document.body,
  })
}

const defaultBackupStatus: BackupStatus = {
  backupDir: '/tmp/mesnet-backups',
  lastBackupAt: null,
  backupCount: 0,
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
  backupStatusMock.mockReset()
  createBackupMock.mockReset()
  restoreBackupMock.mockReset()
  saveDialogMock.mockReset()
  openDialogMock.mockReset()
  openPathMock.mockReset()
  toastAddMock.mockReset()
  confirmRequireMock.mockReset()
  listUsersMock.mockResolvedValue([])
  backupStatusMock.mockResolvedValue(defaultBackupStatus)
  document.body.innerHTML = ''
  // `useAuth` modül düzeyinde tekil durum taşır; testler arasında oturum sızmasın.
  signOut()
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
    { id: 1, name: 'Deniz ARSLAN', isActive: true },
    { id: 2, name: 'Ayşe Kaya', isActive: true },
    { id: 3, name: 'Eski Kullanıcı', isActive: false },
  ]

  it('renders the user table including an inactive user, tagged as such', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)

    const wrapper = mountView()
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('Deniz ARSLAN')
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

describe('SettingsView kullanıcı diyalogları — arka uç reddi form kaybettirmez', () => {
  const sampleUsers: User[] = [
    { id: 1, name: 'Deniz ARSLAN', isActive: true },
    { id: 2, name: 'Ayşe Kaya', isActive: true },
  ]

  it('create reddedilince diyalog açık kalır, girilen değerler korunur', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    createUserMock.mockRejectedValue(new Error('Bu isimde bir kullanıcı zaten var'))

    const wrapper = mountView()
    await flushPromises()
    const addButton = wrapper.findAll('button').find((b) => b.text() === labels.settings.users.add)
    await addButton!.trigger('click')
    await flushPromises()

    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-name')!, 'Ayşe Kaya')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-pin')!, '1234')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-pin-confirm')!, '1234')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-create-save-button"]')!.click()
    await flushPromises()

    // Diyalog hâlâ DOM'da ve girilen ad korunuyor — kaybolmadı.
    expect(document.body.querySelector<HTMLInputElement>('#user-create-name')?.value).toBe('Ayşe Kaya')
    wrapper.unmount()
  })

  it('create başarılı olunca diyalog kapanır', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    createUserMock.mockResolvedValue({ id: 3, name: 'Yeni Kullanıcı', isActive: true })

    const wrapper = mountView()
    await flushPromises()
    const addButton = wrapper.findAll('button').find((b) => b.text() === labels.settings.users.add)
    await addButton!.trigger('click')
    await flushPromises()

    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-name')!, 'Yeni Kullanıcı')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-pin')!, '1234')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-create-pin-confirm')!, '1234')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-create-save-button"]')!.click()
    await flushPromises()

    expect(document.body.querySelector('#user-create-name')).toBeNull()
    wrapper.unmount()
  })

  it('rename reddedilince diyalog açık kalır', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    renameUserMock.mockRejectedValue(new Error('Bu isimde bir kullanıcı zaten var'))

    const wrapper = mountView()
    await flushPromises()
    const renameButtons = wrapper.findAll(`button[aria-label="${labels.settings.users.rename}"]`)
    await renameButtons[0].trigger('click')
    await flushPromises()

    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-rename-name')!, 'Ayşe Kaya')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-rename-save-button"]')!.click()
    await flushPromises()

    expect(document.body.querySelector<HTMLInputElement>('#user-rename-name')?.value).toBe('Ayşe Kaya')
    wrapper.unmount()
  })

  it('rename başarılı olunca diyalog kapanır', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    renameUserMock.mockResolvedValue(undefined)

    const wrapper = mountView()
    await flushPromises()
    const renameButtons = wrapper.findAll(`button[aria-label="${labels.settings.users.rename}"]`)
    await renameButtons[0].trigger('click')
    await flushPromises()

    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-rename-name')!, 'Yeni Ad')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-rename-save-button"]')!.click()
    await flushPromises()

    expect(document.body.querySelector('#user-rename-name')).toBeNull()
    wrapper.unmount()
  })

  it('PIN değişikliği reddedilince diyalog açık kalır', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    setUserPinMock.mockRejectedValue(new Error("PIN 4-6 haneli rakamlardan oluşmalı."))

    const wrapper = mountView()
    await flushPromises()
    const pinButtons = wrapper.findAll(`button[aria-label="${labels.settings.users.changePin}"]`)
    await pinButtons[0].trigger('click')
    await flushPromises()

    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-pin-new')!, '1234')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-pin-new-confirm')!, '1234')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-pin-save-button"]')!.click()
    await flushPromises()

    expect(document.body.querySelector('#user-pin-new')).not.toBeNull()
    wrapper.unmount()
  })

  it('PIN değişikliği başarılı olunca diyalog kapanır', async () => {
    getMock.mockResolvedValue(baseSettings)
    listUsersMock.mockResolvedValue(sampleUsers)
    setUserPinMock.mockResolvedValue(undefined)

    const wrapper = mountView()
    await flushPromises()
    const pinButtons = wrapper.findAll(`button[aria-label="${labels.settings.users.changePin}"]`)
    await pinButtons[0].trigger('click')
    await flushPromises()

    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-pin-new')!, '1234')
    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-pin-new-confirm')!, '1234')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-pin-save-button"]')!.click()
    await flushPromises()

    expect(document.body.querySelector('#user-pin-new')).toBeNull()
    wrapper.unmount()
  })
})

describe('SettingsView — oturumdaki kullanıcı kendini yeniden adlandırınca', () => {
  it('ekran yeni adı gösterir; kendi satırında pasifleştirme kapalı, diğerlerinde açık kalır', async () => {
    getMock.mockResolvedValue(baseSettings)
    let knownUsers: User[] = [
      { id: 1, name: 'Eski Ad', isActive: true },
      { id: 2, name: 'Diğer Kullanıcı', isActive: true },
    ]
    listUsersMock.mockImplementation(() => Promise.resolve(knownUsers))
    vi.mocked(usersApi.login).mockResolvedValue(true)
    renameUserMock.mockImplementation(async (id: number, name: string) => {
      knownUsers = knownUsers.map((user) => (user.id === id ? { ...user, name } : user))
    })

    await signIn(1, '1234')
    expect(currentUser.value?.name).toBe('Eski Ad')

    const wrapper = mountView()
    await flushPromises()

    // Kendi satırında pasifleştirme düğmesi devre dışı, diğer kullanıcıda açık.
    const rows = wrapper.findAll('tbody tr')
    const selfToggle = rows[0].findAll('button')[2]
    const otherToggle = rows[1].findAll('button')[2]
    expect(selfToggle.attributes('disabled')).toBeDefined()
    expect(otherToggle.attributes('disabled')).toBeUndefined()

    const renameButtons = wrapper.findAll(`button[aria-label="${labels.settings.users.rename}"]`)
    await renameButtons[0].trigger('click')
    await flushPromises()

    setNativeValue(document.body.querySelector<HTMLInputElement>('#user-rename-name')!, 'Yeni Ad')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-rename-save-button"]')!.click()
    await flushPromises()

    expect(currentUser.value?.name).toBe('Yeni Ad')
    expect(wrapper.text()).toContain('Yeni Ad')

    wrapper.unmount()
  })
})

function backupButton(wrapper: ReturnType<typeof mountView>, label: string) {
  return wrapper.findAll('button').find((button) => button.text() === label)!
}

describe('SettingsView — yedekleme kartı', () => {
  const statusWithBackups: BackupStatus = {
    backupDir: '/Users/test/MESNET/backups',
    lastBackupAt: '2026-09-20',
    backupCount: 5,
  }

  it('shows the last automatic backup date and count', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)

    const wrapper = mountView()
    await flushPromises()

    // `isoToDate` yerel tarihten kurulur; 'tr-TR' biçimi GG.AA.YYYY döner.
    expect(wrapper.text()).toContain('20.09.2026')
    expect(wrapper.text()).toContain('5')

    wrapper.unmount()
  })

  it('shows the empty-state message when there is no automatic backup yet', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue({ ...defaultBackupStatus, lastBackupAt: null })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain(labels.settings.backup.noBackupYet)

    wrapper.unmount()
  })

  it('creates a backup at the path returned by the save dialog', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)
    saveDialogMock.mockResolvedValue('/Users/test/chosen.db')
    createBackupMock.mockResolvedValue(undefined)

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.createBackup).trigger('click')
    await flushPromises()

    expect(createBackupMock).toHaveBeenCalledWith('/Users/test/chosen.db')
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'success', summary: labels.settings.backup.createBackupSaved }),
    )
    wrapper.unmount()
  })

  it('does nothing when the save dialog is cancelled', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)
    saveDialogMock.mockResolvedValue(null)

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.createBackup).trigger('click')
    await flushPromises()

    expect(createBackupMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('shows the real backend error when create_backup fails', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)
    saveDialogMock.mockResolvedValue('/Users/test/chosen.db')
    createBackupMock.mockRejectedValue(new Error('create_backup: disk dolu'))

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.createBackup).trigger('click')
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'create_backup: disk dolu' }),
    )
    wrapper.unmount()
  })

  it('opens the backup folder through the opener plugin', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.openFolder).trigger('click')
    await flushPromises()

    expect(openPathMock).toHaveBeenCalledWith(statusWithBackups.backupDir)
    wrapper.unmount()
  })

  it('restores from the selected file once the confirmation is accepted', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)
    openDialogMock.mockResolvedValue('/Users/test/chosen-backup.db')
    restoreBackupMock.mockResolvedValue(undefined)
    confirmRequireMock.mockImplementation((options) => options.accept?.())

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.restore).trigger('click')
    await flushPromises()

    expect(confirmRequireMock).toHaveBeenCalledWith(
      expect.objectContaining({ message: labels.settings.backup.restoreConfirmMessage }),
    )
    expect(restoreBackupMock).toHaveBeenCalledWith('/Users/test/chosen-backup.db')
    wrapper.unmount()
  })

  it('does not restore when the confirmation is rejected', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)
    openDialogMock.mockResolvedValue('/Users/test/chosen-backup.db')
    confirmRequireMock.mockImplementation(() => {})

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.restore).trigger('click')
    await flushPromises()

    expect(restoreBackupMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('does not even open the confirmation when the open dialog is cancelled', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)
    openDialogMock.mockResolvedValue(null)

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.restore).trigger('click')
    await flushPromises()

    expect(confirmRequireMock).not.toHaveBeenCalled()
    expect(restoreBackupMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('shows the real backend error when restore_backup fails', async () => {
    getMock.mockResolvedValue(baseSettings)
    backupStatusMock.mockResolvedValue(statusWithBackups)
    openDialogMock.mockResolvedValue('/Users/test/chosen-backup.db')
    restoreBackupMock.mockRejectedValue(new Error('restore_backup: dosya bozuk'))
    confirmRequireMock.mockImplementation((options) => options.accept?.())

    const wrapper = mountView()
    await flushPromises()
    await backupButton(wrapper, labels.settings.backup.restore).trigger('click')
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'restore_backup: dosya bozuk' }),
    )
    wrapper.unmount()
  })
})
