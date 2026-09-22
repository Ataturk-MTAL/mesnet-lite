import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import HelpView from './HelpView.vue'
import { labels } from '../i18n/labels'

function mountView() {
  return mount(HelpView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
  })
}

describe('HelpView', () => {
  it('beklenen bölüm başlıklarını çizer', () => {
    // Arrange & Act
    const wrapper = mountView()
    const text = wrapper.text()

    // Assert
    for (const section of labels.help.sections) {
      expect(text).toContain(section.title)
    }
    wrapper.unmount()
  })

  it('ilk bölümün içeriği açık gelir', () => {
    // Arrange & Act
    const wrapper = mountView()
    const text = wrapper.text()

    // Assert
    for (const paragraph of labels.help.sections[0].paragraphs) {
      expect(text).toContain(paragraph)
    }
    wrapper.unmount()
  })
})
