import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import UpdateDialog from './UpdateDialog.vue'
import { useUpdaterStore } from '../../stores/updater'
import { labels } from '../../i18n/labels'

vi.mock('../../api/updater', () => ({ updaterApi: { isSupported: () => Promise.resolve(true) } }))
const checkMock = vi.fn<() => Promise<unknown>>()
vi.mock('@tauri-apps/plugin-updater', () => ({ check: () => checkMock() }))
const relaunchMock = vi.fn<() => Promise<void>>()
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: () => relaunchMock() }))

const toastAddMock = vi.fn<(message: { severity: string; summary?: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({ useToast: () => ({ add: toastAddMock }) }))

const downloadMock = vi.fn<() => Promise<void>>()

function mountDialog() {
  return mount(UpdateDialog, {
    attachTo: document.body,
    global: { plugins: [[OpenVue, { theme: { preset: Aura } }], ToastService] },
  })
}

async function offerUpdate(body: string | undefined): Promise<void> {
  checkMock.mockResolvedValue({
    version: '9.9.9',
    currentVersion: '0.1.4',
    body,
    downloadAndInstall: () => downloadMock(),
  })
  await useUpdaterStore().checkOnStartup()
  await flushPromises()
}

beforeEach(() => {
  document.body.innerHTML = ''
  checkMock.mockReset()
  toastAddMock.mockReset()
  downloadMock.mockReset().mockResolvedValue(undefined)
  relaunchMock.mockReset().mockResolvedValue(undefined)
})

describe('UpdateDialog', () => {
  it('güncelleme yokken hiçbir şey çizmez', async () => {
    checkMock.mockResolvedValue(null)
    const wrapper = mountDialog()
    await useUpdaterStore().checkOnStartup()
    await flushPromises()

    expect(document.body.textContent).not.toContain(labels.update.updateNow)
    wrapper.unmount()
  })

  it('sürüm, mevcut sürüm, notlar ve yedekleme bilgisini gösterir', async () => {
    const wrapper = mountDialog()
    await offerUpdate('Yeni özellikler')

    const text = document.body.textContent ?? ''
    expect(text).toContain(labels.update.title('9.9.9'))
    expect(text).toContain(labels.update.currentVersion('0.1.4'))
    expect(text).toContain('Yeni özellikler')
    expect(text).toContain(labels.update.backupInfo)
    wrapper.unmount()
  })

  it('sürüm notlarını HTML olarak değil düz metin olarak basar', async () => {
    const wrapper = mountDialog()
    await offerUpdate('<img src=x onerror="alert(1)">')

    expect(document.body.querySelector('[data-testid="update-notes"] img')).toBeNull()
    expect(document.body.querySelector('[data-testid="update-notes"]')?.textContent).toContain(
      '<img src=x',
    )
    wrapper.unmount()
  })

  it('"Sonra" diyaloğu kapatır', async () => {
    const wrapper = mountDialog()
    await offerUpdate('Not')

    document.body.querySelector<HTMLButtonElement>('[data-testid="update-later-button"]')?.click()
    await flushPromises()

    expect(useUpdaterStore().dialogVisible).toBe(false)
    wrapper.unmount()
  })

  it('"Şimdi Güncelle" indirir ve ardından yeniden başlatır', async () => {
    const wrapper = mountDialog()
    await offerUpdate('Not')

    document.body.querySelector<HTMLButtonElement>('[data-testid="update-now-button"]')?.click()
    await flushPromises()

    expect(downloadMock).toHaveBeenCalledTimes(1)
    expect(relaunchMock).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('kurulum hatasında Toast gösterir ve diyalog kapatılabilir kalır', async () => {
    const wrapper = mountDialog()
    await offerUpdate('Not')
    downloadMock.mockRejectedValue(new Error('disk dolu'))

    document.body.querySelector<HTMLButtonElement>('[data-testid="update-now-button"]')?.click()
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'disk dolu' }),
    )
    expect(relaunchMock).not.toHaveBeenCalled()
    expect(useUpdaterStore().installing).toBe(false)
    expect(document.body.querySelector('[data-testid="update-later-button"]')).not.toBeNull()
    wrapper.unmount()
  })
})
