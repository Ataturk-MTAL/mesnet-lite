import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import AppSidebar from './AppSidebar.vue'
import { labels } from '../../i18n/labels'

// RouterLink stub'ı slot içeriğini basmalı; `true` stub'ı basmaz ve menü
// etiketleri kaybolur.
const stubs = {
  // `to` prop olarak bildirildiği için öznitelik olarak düşmez; data-to ile yansıtılır.
  RouterLink: { template: '<a :data-to="to"><slot /></a>', props: ['to'] },
  RouterView: { template: '<div />' },
  Drawer: { template: '<div><slot /></div>' },
  Button: { template: '<button />' },
}

describe('AppSidebar', () => {
  it('tüm ana menü başlıklarını Türkçe olarak gösterir', () => {
    const wrapper = mount(AppSidebar, { global: { stubs } })
    const text = wrapper.text()
    expect(text).toContain(labels.nav.companies)
    expect(text).toContain(labels.nav.students)
    expect(text).toContain(labels.nav.teachers)
    expect(text).toContain(labels.nav.settings)
    expect(text).toContain(labels.nav.importExport)
  })

  it('menü gruplarını Türkçe başlıklarla gösterir', () => {
    const wrapper = mount(AppSidebar, { global: { stubs } })
    const text = wrapper.text()
    expect(text).toContain(labels.nav.groupRecords)
    expect(text).toContain(labels.nav.groupAdmin)
  })

  it('her menü öğesini kendi rotasına bağlar', () => {
    const wrapper = mount(AppSidebar, { global: { stubs } })
    const targets = wrapper.findAll('a').map((a) => a.attributes('data-to'))
    expect(targets).toContain('/companies')
    expect(targets).toContain('/teachers')
    expect(targets).toContain('/import-export')
  })
})
