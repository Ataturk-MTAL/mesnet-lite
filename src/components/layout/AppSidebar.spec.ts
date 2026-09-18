import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import AppSidebar from './AppSidebar.vue'
import { labels } from '../../i18n/labels'

// RouterLink stub'ı slot içeriğini basmalı; `true` stub'ı basmaz ve menü
// etiketleri (SidebarMenuButton as-child ile RouterLink'e devredilir) kaybolur.
const routerStubs = {
  RouterLink: { template: '<a><slot /></a>' },
  RouterView: { template: '<div />' },
}

describe('AppSidebar', () => {
  it('tüm ana menü başlıklarını Türkçe olarak gösterir', () => {
    const wrapper = mount(AppSidebar, { global: { stubs: routerStubs } })
    const text = wrapper.text()
    expect(text).toContain(labels.nav.companies)
    expect(text).toContain(labels.nav.students)
    expect(text).toContain(labels.nav.teachers)
    expect(text).toContain(labels.nav.settings)
    expect(text).toContain(labels.nav.importExport)
  })

  it('menü gruplarını Türkçe başlıklarla gösterir', () => {
    const wrapper = mount(AppSidebar, { global: { stubs: routerStubs } })
    const text = wrapper.text()
    expect(text).toContain(labels.nav.groupRecords)
    expect(text).toContain(labels.nav.groupAdmin)
  })

  it('her menü öğesini kendi rotasına bağlar', () => {
    const wrapper = mount(AppSidebar, { global: { stubs: routerStubs } })
    const hrefs = wrapper.findAll('a').map((a) => a.attributes('to'))
    expect(hrefs).toContain('/companies')
    expect(hrefs).toContain('/teachers')
  })
})
