import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import UserCreateDialog from './UserCreateDialog.vue'
import { labels } from '../../i18n/labels'

// Dialog `body`'ye teleport edilir; `wrapper.find` onu göremez, doğrudan
// `document.body` sorgulanır (CompanyFormDialog.spec.ts'teki desen).

// Toast, bu bileşenin dışında (App.vue'da) yaşar; DOM yerine casusla doğrularız
// (LoginView.spec.ts'teki desen).
const toastAddMock = vi.fn<(message: { severity: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

function mountDialog(saving = false) {
  return mount(UserCreateDialog, {
    props: { visible: true, saving },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

function setInput(id: string, value: string): void {
  const input = document.body.querySelector<HTMLInputElement>(`#${id}`)
  if (!input) throw new Error(`#${id} bulunamadı`)
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function saveButton(): HTMLButtonElement {
  const button = document.body.querySelector<HTMLButtonElement>('[data-testid="user-create-save-button"]')
  if (!button) throw new Error('Kaydet düğmesi bulunamadı')
  return button
}

beforeEach(() => {
  document.body.innerHTML = ''
  toastAddMock.mockReset()
})

describe('UserCreateDialog', () => {
  it('save sonrası kendini kapatmaz — update:visible false yayınlanmaz', async () => {
    // Arrange
    const wrapper = mountDialog()
    await nextTick()
    setInput('user-create-name', 'Yeni Kullanıcı')
    setInput('user-create-pin', '1234')
    setInput('user-create-pin-confirm', '1234')
    await nextTick()

    // Act
    saveButton().click()
    await nextTick()

    // Assert
    expect(wrapper.emitted('save')).toBeTruthy()
    expect(wrapper.emitted('update:visible')).toBeFalsy()
  })

  it('saving true iken kaydet düğmesi devre dışı kalır (loading)', async () => {
    // Arrange & Act
    mountDialog(true)
    await nextTick()

    // Assert
    expect(saveButton().disabled).toBe(true)
  })

  it('saving true iken form alanları devre dışı kalır', async () => {
    // Arrange & Act
    mountDialog(true)
    await nextTick()

    // Assert
    expect(document.body.querySelector<HTMLInputElement>('#user-create-name')?.disabled).toBe(true)
  })

  it('iptal her zaman kapatır', async () => {
    // Arrange
    const wrapper = mountDialog()
    await nextTick()

    // Act
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-create-cancel-button"]')!.click()
    await nextTick()

    // Assert
    expect(wrapper.emitted('update:visible')?.[0]).toEqual([false])
  })

  it('PIN tekrarı uyuşmazsa kaydetmez, Türkçe uyarı gösterir', async () => {
    // Arrange
    const wrapper = mountDialog()
    await nextTick()
    setInput('user-create-name', 'Yeni Kullanıcı')
    setInput('user-create-pin', '1234')
    setInput('user-create-pin-confirm', '5678')
    await nextTick()

    // Act
    saveButton().click()
    await nextTick()

    // Assert
    expect(wrapper.emitted('save')).toBeFalsy()
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'warn', detail: labels.auth.pinMismatch }),
    )
  })
})
