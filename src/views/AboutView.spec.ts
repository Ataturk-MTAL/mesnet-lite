import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import AboutView from './AboutView.vue'
import { labels } from '../i18n/labels'

const openUrlMock = vi.fn<(url: string) => Promise<void>>()
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: (url: string) => openUrlMock(url),
}))

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
})
