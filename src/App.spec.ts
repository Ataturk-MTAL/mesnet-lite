import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import App from './App.vue'
import { useAuthStore } from './stores/auth'
import { useTermStore } from './stores/term'

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
  // Dönem listesi Tauri komutlarından gelir; testte backend yoktur (AppSidebar bunu okur).
  const termStore = useTermStore()
  termStore.activeTerm = '2026-2027/1'
  termStore.terms = ['2026-2027/1']
  vi.spyOn(termStore, 'loadTerms').mockResolvedValue(undefined)
})

// Oturum Pinia store'unda paylaşılır; testler arasında sızmaması için sıfırlanır.
afterEach(() => {
  useAuthStore().signOut()
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
    const { usersApi } = await import('./api/users')
    vi.mocked(usersApi.login).mockResolvedValue(true)
    vi.mocked(usersApi.list).mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    await useAuthStore().signIn(1, '1234')
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.find('.sidebar').exists()).toBe(true)

    wrapper.unmount()
  })

  it('oturum kapanınca seçim store’u sıfırlanır, başka kullanıcı önceki filtreleri devralmaz', async () => {
    const { usersApi } = await import('./api/users')
    const { useSelectionStore } = await import('./stores/selection')
    vi.mocked(usersApi.login).mockResolvedValue(true)
    vi.mocked(usersApi.list).mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    const authStore = useAuthStore()
    await authStore.signIn(1, '1234')
    const wrapper = mountApp()
    await flushPromises()

    const selection = useSelectionStore()
    selection.selectedTeacherId = 7
    selection.studentSearch = 'ayşe'

    authStore.signOut()
    await flushPromises()

    expect(selection.selectedTeacherId).toBeNull()
    expect(selection.studentSearch).toBe('')

    wrapper.unmount()
  })
})
