import { beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import UserRenameDialog from './UserRenameDialog.vue'
import type { User } from '../../types/models'

// Dialog `body`'ye teleport edilir; `wrapper.find` onu göremez, doğrudan
// `document.body` sorgulanır (CompanyFormDialog.spec.ts'teki desen).

const sampleUser: User = { id: 1, name: 'Hakan GÜLEN', isActive: true }

function mountDialog(saving = false, user: User | null = sampleUser) {
  return mount(UserRenameDialog, {
    props: { visible: true, user, saving },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

function nameInput(): HTMLInputElement {
  const input = document.body.querySelector<HTMLInputElement>('#user-rename-name')
  if (!input) throw new Error('#user-rename-name bulunamadı')
  return input
}

function setInput(id: string, value: string): void {
  const input = document.body.querySelector<HTMLInputElement>(`#${id}`)
  if (!input) throw new Error(`#${id} bulunamadı`)
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function saveButton(): HTMLButtonElement {
  const button = document.body.querySelector<HTMLButtonElement>('[data-testid="user-rename-save-button"]')
  if (!button) throw new Error('Kaydet düğmesi bulunamadı')
  return button
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('UserRenameDialog', () => {
  it('save sonrası kendini kapatmaz — update:visible false yayınlanmaz', async () => {
    // Arrange
    const wrapper = mountDialog()
    await nextTick()
    setInput('user-rename-name', 'Yeni Ad')
    await nextTick()

    // Act
    saveButton().click()
    await nextTick()

    // Assert
    expect(wrapper.emitted('save')).toEqual([['Yeni Ad']])
    expect(wrapper.emitted('update:visible')).toBeFalsy()
  })

  it('saving true iken kaydet düğmesi devre dışı kalır (loading)', async () => {
    // Arrange & Act
    mountDialog(true)
    await nextTick()

    // Assert
    expect(saveButton().disabled).toBe(true)
  })

  it('saving true iken ad alanı devre dışı kalır', async () => {
    // Arrange & Act
    mountDialog(true)
    await nextTick()

    // Assert
    expect(nameInput().disabled).toBe(true)
  })

  it('iptal her zaman kapatır', async () => {
    // Arrange
    const wrapper = mountDialog()
    await nextTick()

    // Act
    document.body.querySelector<HTMLButtonElement>('[data-testid="user-rename-cancel-button"]')!.click()
    await nextTick()

    // Assert
    expect(wrapper.emitted('update:visible')?.[0]).toEqual([false])
  })
})
