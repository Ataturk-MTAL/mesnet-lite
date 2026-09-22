import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import AboutView from './AboutView.vue'
import { labels } from '../i18n/labels'

function mountView() {
  return mount(AboutView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
  })
}

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
})
