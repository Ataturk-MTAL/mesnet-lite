import { beforeEach, describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import AppSidebar from './AppSidebar.vue'
import { labels } from '../../i18n/labels'
import { useTermStore } from '../../stores/term'

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

beforeEach(() => {
  // Dönem listesi Tauri komutlarından gelir; testte backend yoktur. Store
  // önceden doldurulur, `onMounted`'daki gerçek `loadTerms` API'ye gitmesin
  // diye casuslanır.
  const termStore = useTermStore()
  termStore.activeTerm = '2026-2027/1'
  termStore.terms = ['2026-2027/1']
  vi.spyOn(termStore, 'loadTerms').mockResolvedValue(undefined)
})

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

  it('okul amblemini anlamlı alt metinle başlıkta gösterir', () => {
    const wrapper = mountSidebar()
    const logo = wrapper.find('img.brand-logo')
    expect(logo.exists()).toBe(true)
    expect(logo.attributes('alt')).toBe(labels.app.logoAlt)
  })
})
