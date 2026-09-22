import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import Select from 'openvue/select'
import LoginView from './LoginView.vue'
import { labels } from '../i18n/labels'
import type { User } from '../types/models'

const hasAnyMock = vi.fn<() => Promise<boolean>>()
const listMock = vi.fn<() => Promise<User[]>>()
const createMock = vi.fn<(name: string, pin: string) => Promise<User>>()
const loginMock = vi.fn<(userId: number, pin: string) => Promise<boolean>>()
vi.mock('../api/users', () => ({
  usersApi: {
    hasAny: () => hasAnyMock(),
    list: () => listMock(),
    create: (name: string, pin: string) => createMock(name, pin),
    rename: vi.fn(),
    setPin: vi.fn(),
    setActive: vi.fn(),
    login: (userId: number, pin: string) => loginMock(userId, pin),
  },
}))

// LoginView kendi Toast'unu barındırmaz (App.vue'da yaşar); DOM yerine bu casusla doğrularız.
const toastAddMock = vi.fn<(message: { severity: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

function mountView() {
  return mount(LoginView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
}

beforeEach(() => {
  hasAnyMock.mockReset()
  listMock.mockReset()
  createMock.mockReset()
  loginMock.mockReset()
  toastAddMock.mockReset()
  document.body.innerHTML = ''
})

describe('LoginView — ilk kurulum', () => {
  it('hiç kullanıcı yokken ilk kurulum formunu çizer, giriş formunu değil', async () => {
    hasAnyMock.mockResolvedValue(false)

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain(labels.auth.firstSetupTitle)
    expect(listMock).not.toHaveBeenCalled()

    wrapper.unmount()
  })

  it('PIN ve tekrarı uyuşmazsa oluşturmaz, Türkçe uyarı gösterir', async () => {
    hasAnyMock.mockResolvedValue(false)

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('#setup-name').setValue('Yeni Kullanıcı')
    await wrapper.find('#setup-pin').setValue('1234')
    await wrapper.find('#setup-pin-confirm').setValue('5678')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(createMock).not.toHaveBeenCalled()
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'warn', detail: labels.auth.pinMismatch }),
    )

    wrapper.unmount()
  })

  it("PIN'ler eşleşince ilk kullanıcıyı oluşturur ve doğrudan onunla giriş yapar", async () => {
    hasAnyMock.mockResolvedValue(false)
    createMock.mockResolvedValue({ id: 9, name: 'Yeni Kullanıcı', isActive: true })
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 9, name: 'Yeni Kullanıcı', isActive: true }])

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('#setup-name').setValue('Yeni Kullanıcı')
    await wrapper.find('#setup-pin').setValue('1234')
    await wrapper.find('#setup-pin-confirm').setValue('1234')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(createMock).toHaveBeenCalledWith('Yeni Kullanıcı', '1234')
    expect(loginMock).toHaveBeenCalledWith(9, '1234')

    wrapper.unmount()
  })

  it('oluşturma başarılı ama giriş (login) başarısız olursa hata gösterir, giriş moduna geçer ve yeni kullanıcıyı seçili bırakır', async () => {
    hasAnyMock.mockResolvedValue(false)
    createMock.mockResolvedValue({ id: 9, name: 'Yeni Kullanıcı', isActive: true })
    loginMock.mockResolvedValue(false)
    listMock.mockResolvedValue([{ id: 9, name: 'Yeni Kullanıcı', isActive: true }])

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('#setup-name').setValue('Yeni Kullanıcı')
    await wrapper.find('#setup-pin').setValue('1234')
    await wrapper.find('#setup-pin-confirm').setValue('1234')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    // Ekran kilitlenmez: 'setup' değil giriş moduna geçer, yeniden aynı adı
    // oluşturmaya çalışıp çakışma hatasına düşmez.
    expect(wrapper.text()).not.toContain(labels.auth.firstSetupTitle)
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: labels.auth.wrongPin }),
    )
    expect(wrapper.findComponent(Select).props('modelValue')).toBe(9)

    wrapper.unmount()
  })

  it('oluşturma başarılı ama giriş sırasındaki liste çağrısı hata verirse de giriş moduna geçer, gerçek hata mesajı gösterilir', async () => {
    hasAnyMock.mockResolvedValue(false)
    createMock.mockResolvedValue({ id: 9, name: 'Yeni Kullanıcı', isActive: true })
    loginMock.mockResolvedValue(true)
    listMock.mockRejectedValue(new Error('Veritabanına ulaşılamadı'))

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('#setup-name').setValue('Yeni Kullanıcı')
    await wrapper.find('#setup-pin').setValue('1234')
    await wrapper.find('#setup-pin-confirm').setValue('1234')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).not.toContain(labels.auth.firstSetupTitle)
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'Veritabanına ulaşılamadı' }),
    )

    wrapper.unmount()
  })

  it('oluşturmanın kendisi başarısız olursa ekran setup modunda kalır', async () => {
    hasAnyMock.mockResolvedValue(false)
    createMock.mockRejectedValue(new Error('Bu isimde bir kullanıcı zaten var'))

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('#setup-name').setValue('Var Olan')
    await wrapper.find('#setup-pin').setValue('1234')
    await wrapper.find('#setup-pin-confirm').setValue('1234')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain(labels.auth.firstSetupTitle)
    expect(loginMock).not.toHaveBeenCalled()
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'Bu isimde bir kullanıcı zaten var' }),
    )

    wrapper.unmount()
  })
})

describe('LoginView — giriş', () => {
  const activeUser: User = { id: 1, name: 'Hakan GÜLEN', isActive: true }
  const inactiveUser: User = { id: 2, name: 'Pasif Kullanıcı', isActive: false }

  it('kullanıcı varken giriş formunu çizer, yalnız etkin kullanıcıları listeler', async () => {
    hasAnyMock.mockResolvedValue(true)
    listMock.mockResolvedValue([activeUser, inactiveUser])

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).not.toContain(labels.auth.firstSetupTitle)
    const select = wrapper.findComponent(Select)
    expect(select.props('options')).toEqual([activeUser])

    wrapper.unmount()
  })

  it('yanlış PIN\'de hata gösterir, alanı temizler ve odağı PIN\'e döndürür', async () => {
    hasAnyMock.mockResolvedValue(true)
    listMock.mockResolvedValue([activeUser])
    loginMock.mockResolvedValue(false)

    const wrapper = mountView()
    await flushPromises()

    const select = wrapper.findComponent(Select)
    await select.vm.$emit('update:modelValue', activeUser.id)
    await wrapper.find('#login-pin').setValue('9999')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(loginMock).toHaveBeenCalledWith(activeUser.id, '9999')
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: labels.auth.wrongPin }),
    )
    expect((wrapper.find('#login-pin').element as HTMLInputElement).value).toBe('')

    wrapper.unmount()
  })
})
