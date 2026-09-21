import { beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import CompanyFormDialog from './CompanyFormDialog.vue'
import { labels } from '../../i18n/labels'
import type { Company, NewCompany } from '../../types/models'

// Dialog `body`'ye teleport edildiğinden `wrapper.find` onu göremez;
// TeacherFormDialog.spec.ts'teki desende olduğu gibi doğrudan `document.body` sorgulanır.

function companyFixture(overrides: Partial<Company> = {}): Company {
  return {
    id: 1,
    name: 'Akdeniz Elektronik San. Tic. Ltd. Şti.',
    contactFirstName: 'Ali',
    contactLastName: 'Veli',
    phone: '',
    email: '',
    addressText: 'Karaduvar Mah. Serbest Bölge 14. Cadde No:13 Akdeniz/Mersin',
    district: 'Akdeniz',
    latitude: null,
    longitude: null,
    geocodeStatus: 'pending',
    oneWayDistanceKm: 12.4,
    notes: '',
    createdAt: '2026-01-01',
    updatedAt: '2026-01-01',
    ...overrides,
  }
}

function mountDialog(company: Company | null) {
  return mount(CompanyFormDialog, {
    props: { visible: true, company },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

function districtInput(): HTMLInputElement {
  const input = document.body.querySelector<HTMLInputElement>('#company-district')
  if (!input) throw new Error('#company-district bulunamadı')
  return input
}

function setInput(id: string, value: string): void {
  const input = document.body.querySelector<HTMLInputElement | HTMLTextAreaElement>(`#${id}`)
  if (!input) throw new Error(`#${id} bulunamadı`)
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function saveButton(): HTMLButtonElement {
  const buttons = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
  const button = buttons.find((b) => b.textContent?.trim() === labels.common.save)
  if (!button) throw new Error('Kaydet düğmesi bulunamadı')
  return button
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('CompanyFormDialog ilçe alanı', () => {
  it('boş formda İlçe alanının etiketini ve ipucunu gösterir', async () => {
    // Arrange & Act
    mountDialog(null)
    await nextTick()

    // Assert
    expect(document.body.textContent).toContain(labels.company.district)
    expect(document.body.textContent).toContain(labels.company.districtHint)
  })

  it('mevcut kayıttaki ilçe değerini alanda gösterir', async () => {
    // Arrange & Act
    mountDialog(companyFixture({ district: 'Toroslar' }))
    await nextTick()

    // Assert
    expect(districtInput().value).toBe('Toroslar')
  })

  it('ilçe alanı düzenlendiğinde kaydet ile birlikte gönderilir', async () => {
    // Arrange
    const wrapper = mountDialog(companyFixture({ district: 'Akdeniz' }))
    await nextTick()

    // Act
    setInput('company-district', 'Mezitli')
    await nextTick()
    saveButton().click()
    await nextTick()

    // Assert
    const emitted = wrapper.emitted('save')
    expect(emitted).toBeTruthy()
    const savedInput = emitted?.[0]?.[0] as NewCompany
    expect(savedInput.district).toBe('Mezitli')
  })

  it('yeni kayıtta ilçe boş bırakılabilir; boş dize olarak gönderilir', async () => {
    // Arrange
    const wrapper = mountDialog(null)
    await nextTick()
    setInput('company-name', 'Yeni İşletme')
    setInput('company-address', 'Adres')
    await nextTick()

    // Act
    saveButton().click()
    await nextTick()

    // Assert
    const emitted = wrapper.emitted('save')
    const savedInput = emitted?.[0]?.[0] as NewCompany
    expect(savedInput.district).toBe('')
  })
})
