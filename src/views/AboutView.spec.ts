import { describe, it, expect, vi, beforeEach } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import AboutView from './AboutView.vue'
import { labels } from '../i18n/labels'

const openUrlMock = vi.fn<(url: string) => Promise<void>>()
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: (url: string) => openUrlMock(url),
}))

const toastAddMock = vi.fn<(message: { severity: string; summary?: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({ useToast: () => ({ add: toastAddMock }) }))

const isSupportedMock = vi.fn<() => Promise<boolean>>()
vi.mock('../api/updater', () => ({
  updaterApi: { isSupported: () => isSupportedMock() },
}))

const checkMock = vi.fn<() => Promise<unknown>>()
vi.mock('@tauri-apps/plugin-updater', () => ({ check: () => checkMock() }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: vi.fn() }))

const getVersionMock = vi.fn<() => Promise<string>>()
vi.mock('@tauri-apps/api/app', () => ({ getVersion: () => getVersionMock() }))

function mountView() {
  return mount(AboutView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
  })
}

beforeEach(() => {
  openUrlMock.mockReset()
  openUrlMock.mockResolvedValue(undefined)
  isSupportedMock.mockReset()
  isSupportedMock.mockResolvedValue(false)
  checkMock.mockReset()
  getVersionMock.mockReset()
  getVersionMock.mockResolvedValue('0.1.5')
  toastAddMock.mockReset()
})

describe('AboutView', () => {
  it('okul adını ve alan adını birebir gösterir', () => {
    // Arrange & Act
    const wrapper = mountView()

    // Assert
    expect(wrapper.text()).toContain('Atatürk Mesleki ve Teknik Anadolu Lisesi')
    expect(wrapper.text()).toContain('Elektrik Elektronik Teknolojileri Alanı')
    expect(wrapper.text()).toContain(labels.about.school)
    expect(wrapper.text()).toContain(labels.about.field)
    wrapper.unmount()
  })

  it('uygulama adını ve amaç cümlesini gösterir', () => {
    // Arrange & Act
    const wrapper = mountView()

    // Assert
    expect(wrapper.text()).toContain(labels.about.appName)
    expect(wrapper.text()).toContain(labels.about.purpose)
    wrapper.unmount()
  })

  it('okul amblemini anlamlı alt metinle gösterir', () => {
    // Arrange & Act
    const wrapper = mountView()

    // Assert
    const logo = wrapper.find('img')
    expect(logo.exists()).toBe(true)
    expect(logo.attributes('alt')).toBe(labels.app.logoAlt)
    wrapper.unmount()
  })

  it('lisans satırını ve amblemin lisans kapsamı dışında olduğu notunu gösterir', () => {
    // Arrange & Act
    const wrapper = mountView()

    // Assert
    expect(wrapper.text()).toContain(labels.about.license)
    expect(wrapper.text()).toContain(labels.about.logoLicenseNote)
    wrapper.unmount()
  })

  it('kaynak kod düğmesine tıklanınca depo adresini tarayıcıda açar', async () => {
    // Arrange
    const wrapper = mountView()

    // Act
    await wrapper.find('button').trigger('click')

    // Assert
    expect(openUrlMock).toHaveBeenCalledWith(labels.about.sourceCodeUrl)
    wrapper.unmount()
  })

  it('güncelleme desteklenmiyorsa "Güncellemeleri Denetle" düğmesini göstermez', async () => {
    // Arrange & Act
    const wrapper = mountView()
    await flushPromises()

    // Assert
    expect(wrapper.find('[data-testid="about-check-updates"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('güncelleme destekleniyorsa düğmeyi gösterir; güncelse bildirim çıkarır', async () => {
    // Arrange
    isSupportedMock.mockResolvedValue(true)
    checkMock.mockResolvedValue(null)
    const wrapper = mountView()
    await flushPromises()

    // Act
    await wrapper.find('[data-testid="about-check-updates"]').trigger('click')
    await flushPromises()

    // Assert
    expect(checkMock).toHaveBeenCalledTimes(1)
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'info', detail: labels.update.upToDate }),
    )
    wrapper.unmount()
  })

  it('uygulama sürümünü gösterir', async () => {
    // Arrange & Act
    const wrapper = mountView()
    await flushPromises()

    // Assert
    expect(wrapper.find('[data-testid="about-version"]').text()).toBe('Sürüm 0.1.5')
    wrapper.unmount()
  })

  it('sürüm okunamazsa sayfa yine çizilir, sürüm metni olmaz', async () => {
    // Arrange
    getVersionMock.mockRejectedValue(new Error('tauri yok'))
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    // Act
    const wrapper = mountView()
    await flushPromises()

    // Assert
    expect(wrapper.text()).toContain(labels.about.appName)
    expect(wrapper.find('[data-testid="about-version"]').exists()).toBe(false)
    expect(warnSpy).toHaveBeenCalled()
    warnSpy.mockRestore()
    wrapper.unmount()
  })
})
