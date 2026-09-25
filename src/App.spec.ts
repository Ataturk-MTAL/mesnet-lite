import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import App from './App.vue'
import { signOut } from './composables/useAuth'

// Dönem listesi Tauri komutlarından gelir; testte backend yoktur (AppSidebar bunu okur).
vi.mock('./composables/useTerm', () => ({
  activeTerm: { value: '2026-2027/1' },
  terms: { value: ['2026-2027/1'] },
  loadTerms: vi.fn().mockResolvedValue(undefined),
  setActiveTerm: vi.fn().mockResolvedValue(undefined),
}))

// LoginView açılışta has_any_user çağırır; giriş yapılmadığı sürece bu yeterli.
const hasAnyMock = vi.fn<() => Promise<boolean>>()
vi.mock('./api/users', () => ({
  usersApi: {
    hasAny: () => hasAnyMock(),
    list: vi.fn(),
    create: vi.fn(),
    rename: vi.fn(),
    setPin: vi.fn(),
    setActive: vi.fn(),
    login: vi.fn(),
  },
}))

function mountApp() {
  return mount(App, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService, ConfirmationService],
      stubs: {
        RouterLink: { template: '<a><slot /></a>' },
        RouterView: { template: '<div />' },
      },
    },
  })
}

beforeEach(() => {
  hasAnyMock.mockReset()
  hasAnyMock.mockResolvedValue(false)
})

// Oturum modül düzeyinde paylaşılır; testler arasında sızmaması için sıfırlanır.
afterEach(() => {
  signOut()
})

describe('App', () => {
  it('giriş yapılmamışken kenar çubuğunu çizmez, giriş ekranını çizer', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.find('.sidebar').exists()).toBe(false)
    expect(wrapper.find('.topbar').exists()).toBe(false)
    expect(wrapper.text()).toContain('MESNET.Lite')

    wrapper.unmount()
  })

  it('giriş yapılınca kenar çubuğunu çizer', async () => {
    const { signIn } = await import('./composables/useAuth')
    const { usersApi } = await import('./api/users')
    vi.mocked(usersApi.login).mockResolvedValue(true)
    vi.mocked(usersApi.list).mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    await signIn(1, '1234')
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.find('.sidebar').exists()).toBe(true)

    wrapper.unmount()
  })

  it('oturum kapanınca seçim store’u sıfırlanır, başka kullanıcı önceki filtreleri devralmaz', async () => {
    const { signIn } = await import('./composables/useAuth')
    const { usersApi } = await import('./api/users')
    const { useSelectionStore } = await import('./stores/selection')
    vi.mocked(usersApi.login).mockResolvedValue(true)
    vi.mocked(usersApi.list).mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    await signIn(1, '1234')
    const wrapper = mountApp()
    await flushPromises()

    const selection = useSelectionStore()
    selection.selectedTeacherId = 7
    selection.studentSearch = 'ayşe'

    signOut()
    await flushPromises()

    expect(selection.selectedTeacherId).toBeNull()
    expect(selection.studentSearch).toBe('')

    wrapper.unmount()
  })
})
