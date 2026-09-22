import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import AppSidebar from './AppSidebar.vue'
import { labels } from '../../i18n/labels'

// Dönem listesi Tauri komutlarından gelir; testte backend yoktur.
vi.mock('../../composables/useTerm', () => ({
  activeTerm: { value: '2026-2027/1' },
  terms: { value: ['2026-2027/1'] },
  loadTerms: vi.fn().mockResolvedValue(undefined),
  setActiveTerm: vi.fn().mockResolvedValue(undefined),
}))

// OpenVue bileşenleri yapılandırma enjeksiyonuna ihtiyaç duyar; stub yerine
// gerçek eklentiyi kurmak bileşenlerin gerçekten render olduğunu da doğrular.
function mountSidebar() {
  return mount(AppSidebar, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
      stubs: {
        // `to` prop olarak bildirildiği için öznitelik olarak düşmez;
        // data-to ile yansıtılır.
        RouterLink: { template: '<a :data-to="to"><slot /></a>', props: ['to'] },
        RouterView: { template: '<div />' },
      },
    },
  })
}

describe('AppSidebar', () => {
  it('tüm ana menü başlıklarını Türkçe olarak gösterir', () => {
    const text = mountSidebar().text()
    expect(text).toContain(labels.nav.companies)
    expect(text).toContain(labels.nav.students)
    expect(text).toContain(labels.nav.teachers)
    expect(text).toContain(labels.nav.settings)
    expect(text).toContain(labels.nav.importExport)
  })

  it('menü gruplarını Türkçe başlıklarla gösterir', () => {
    const text = mountSidebar().text()
    expect(text).toContain(labels.nav.groupRecords)
    expect(text).toContain(labels.nav.groupAdmin)
  })

  it('her menü öğesini kendi rotasına bağlar', () => {
    const targets = mountSidebar()
      .findAll('a')
      .map((a) => a.attributes('data-to'))
    expect(targets).toContain('/companies')
    expect(targets).toContain('/teachers')
    expect(targets).toContain('/import-export')
  })

  it('yardım ve hakkında menü öğelerini gösterir', () => {
    const wrapper = mountSidebar()
    const text = wrapper.text()
    expect(text).toContain(labels.nav.groupHelp)
    expect(text).toContain(labels.nav.help)
    expect(text).toContain(labels.nav.about)

    const targets = wrapper.findAll('a').map((a) => a.attributes('data-to'))
    expect(targets).toContain('/help')
    expect(targets).toContain('/about')
  })
})
