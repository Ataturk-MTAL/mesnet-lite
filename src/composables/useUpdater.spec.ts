import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import { useUpdater } from './useUpdater'
import { useUpdaterStore } from '../stores/updater'
import { labels } from '../i18n/labels'

const isSupportedMock = vi.fn<() => Promise<boolean>>()
vi.mock('../api/updater', () => ({ updaterApi: { isSupported: () => isSupportedMock() } }))

const checkMock = vi.fn<() => Promise<unknown>>()
vi.mock('@tauri-apps/plugin-updater', () => ({ check: () => checkMock() }))

const relaunchMock = vi.fn<() => Promise<void>>()
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: () => relaunchMock() }))

const toastAddMock = vi.fn<(message: { severity: string; summary?: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({ useToast: () => ({ add: toastAddMock }) }))

type ProgressEvent =
  | { event: 'Started'; data: { contentLength?: number } }
  | { event: 'Progress'; data: { chunkLength: number } }
  | { event: 'Finished' }

function fakeUpdate(downloadAndInstall: (cb: (e: ProgressEvent) => void) => Promise<void>) {
  return { version: '9.9.9', currentVersion: '0.1.4', body: 'Notlar', downloadAndInstall }
}

function mountComposable() {
  let api!: ReturnType<typeof useUpdater>
  const Host = defineComponent({
    setup() {
      api = useUpdater()
      return () => null
    },
  })
  const wrapper = mount(Host, {
    global: { plugins: [[OpenVue, { theme: { preset: Aura } }], ToastService] },
  })
  return { wrapper, api }
}

beforeEach(() => {
  isSupportedMock.mockReset().mockResolvedValue(true)
  checkMock.mockReset()
  toastAddMock.mockReset()
  relaunchMock.mockReset().mockResolvedValue(undefined)
  vi.spyOn(console, 'error').mockImplementation(() => undefined)
})

describe('useUpdater', () => {
  it('check null dönerse diyalog açılmaz', async () => {
    checkMock.mockResolvedValue(null)
    const { api } = mountComposable()

    await api.checkOnStartup()

    expect(checkMock).toHaveBeenCalledTimes(1)
    expect(api.dialogVisible.value).toBe(false)
  })

  it('güncelleme varsa diyalog açılır ve sürüm bilgisi tutulur', async () => {
    checkMock.mockResolvedValue(fakeUpdate(async () => undefined))
    const { api } = mountComposable()

    await api.checkOnStartup()

    expect(api.dialogVisible.value).toBe(true)
    expect(api.pending.value?.version).toBe('9.9.9')
  })

  it('"Sonra" sonrası aynı oturumda açılış denetimi tekrar sormaz', async () => {
    checkMock.mockResolvedValue(fakeUpdate(async () => undefined))
    const { api } = mountComposable()
    await api.checkOnStartup()

    api.dismiss()
    await api.checkOnStartup()

    expect(api.dialogVisible.value).toBe(false)
    expect(checkMock).toHaveBeenCalledTimes(1)
  })

  it('"Şimdi Güncelle" önce downloadAndInstall, sonra relaunch çağırır', async () => {
    const order: string[] = []
    checkMock.mockResolvedValue(
      fakeUpdate(async (cb) => {
        cb({ event: 'Started', data: { contentLength: 200 } })
        cb({ event: 'Progress', data: { chunkLength: 100 } })
        order.push('install')
      }),
    )
    relaunchMock.mockImplementation(async () => {
      order.push('relaunch')
    })
    const { api } = mountComposable()
    await api.checkOnStartup()

    await api.installNow()

    expect(order).toEqual(['install', 'relaunch'])
    expect(api.progressPercent.value).toBe(50)
    expect(api.installing.value).toBe(false)
  })

  it('contentLength yoksa ilerleme belirsiz kalır (null)', async () => {
    checkMock.mockResolvedValue(
      fakeUpdate(async (cb) => {
        cb({ event: 'Started', data: {} })
        cb({ event: 'Progress', data: { chunkLength: 10 } })
      }),
    )
    const { api } = mountComposable()
    await api.checkOnStartup()

    await api.installNow()

    expect(api.progressPercent.value).toBeNull()
  })

  it('kurulum hatasında Toast gösterir, relaunch çağırmaz, diyalog açık kalır', async () => {
    checkMock.mockResolvedValue(
      fakeUpdate(async () => {
        throw new Error('imza doğrulanamadı')
      }),
    )
    const { wrapper, api } = mountComposable()
    await api.checkOnStartup()

    await api.installNow()
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'imza doğrulanamadı' }),
    )
    expect(relaunchMock).not.toHaveBeenCalled()
    expect(api.dialogVisible.value).toBe(true)
    expect(api.installing.value).toBe(false)
    wrapper.unmount()
  })

  it('updater_supported false ise check hiç çağrılmaz', async () => {
    isSupportedMock.mockResolvedValue(false)
    const { api } = mountComposable()

    await api.checkOnStartup()

    expect(checkMock).not.toHaveBeenCalled()
    expect(api.supported.value).toBe(false)
  })

  it('açılış denetiminde ağ hatası sessizce konsola yazılır, diyalog açılmaz', async () => {
    checkMock.mockRejectedValue(new Error('ağ yok'))
    const { api } = mountComposable()

    await api.checkOnStartup()

    expect(api.dialogVisible.value).toBe(false)
    expect(console.error).toHaveBeenCalled()
  })

  it('elle denetimde hata olursa store yükseltir, composable Toast gösterir', async () => {
    checkMock.mockRejectedValue(new Error('ağ yok'))
    const { api } = mountComposable()

    await expect(useUpdaterStore().checkManually()).rejects.toThrow('ağ yok')
    await api.checkManually()
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', summary: labels.update.checkFailed, detail: 'ağ yok' }),
    )
  })
})
