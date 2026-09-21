import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import CompaniesView from './CompaniesView.vue'
import { labels } from '../i18n/labels'
import type { Company } from '../types/models'
import type { SettingsMap } from '../api/settings'
import type { GeocodeSummary } from '../api/files'

const listMock = vi.fn<() => Promise<Company[]>>()
vi.mock('../api/companies', () => ({
  companiesApi: {
    list: () => listMock(),
    get: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    remove: vi.fn(),
    setLocation: vi.fn(),
  },
}))

const settingsGetMock = vi.fn<() => Promise<SettingsMap>>()
vi.mock('../api/settings', () => ({
  settingsApi: {
    get: () => settingsGetMock(),
    save: vi.fn(),
    setSchoolLocation: vi.fn(),
  },
}))

vi.mock('../api/files', () => ({
  filesApi: {
    geocodePending: vi.fn<() => Promise<GeocodeSummary>>(),
  },
}))

// Harita Leaflet çizer; bu testin konusu değildir.
const LocationPickerMapStub = { template: '<div />' }

function companyFixture(overrides: Partial<Company> = {}): Company {
  return {
    id: 1,
    name: 'KALEKİM AŞ.',
    contactFirstName: 'Ali',
    contactLastName: 'Veli',
    phone: '0532 000 00 00',
    email: 'ali@example.com',
    addressText: 'Karaduvar Mah. Serbest Bölge 14. Cadde No:13 Akdeniz/Mersin',
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

async function mountView(companies: Company[]): Promise<VueWrapper> {
  listMock.mockResolvedValue(companies)
  settingsGetMock.mockResolvedValue({})
  const wrapper = mount(CompaniesView, {
    global: {
      plugins: [
        [OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }],
        ToastService,
        ConfirmationService,
      ],
      directives: { tooltip: Tooltip },
      stubs: { LocationPickerMap: LocationPickerMapStub },
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  listMock.mockReset()
  settingsGetMock.mockReset()
  document.body.replaceChildren()
})

describe('CompaniesView tablo sütunları', () => {
  it('Adres sütununu göstermez, Yetkili sütununu gösterir', async () => {
    // Arrange & Act
    const wrapper = await mountView([companyFixture()])

    // Assert
    const headers = wrapper.findAll('th').map((th) => th.text())
    expect(headers).not.toContain(labels.company.address)
    expect(headers).toContain(labels.company.contact)
    wrapper.unmount()
  })

  it('yetkili adı ve soyadını tek hücrede birleştirir', async () => {
    // Arrange & Act
    const wrapper = await mountView([companyFixture({ contactFirstName: 'Mehmet Ali', contactLastName: 'Yılmazoğlu' })])

    // Assert
    expect(wrapper.text()).toContain('Mehmet Ali Yılmazoğlu')
    wrapper.unmount()
  })

  it('yetkilisi boş olan işletme için hücreyi tire ile gösterir', async () => {
    // Arrange & Act
    const wrapper = await mountView([companyFixture({ contactFirstName: '', contactLastName: '' })])

    // Assert
    const rows = wrapper.findAll('tbody tr')
    expect(rows[0].text()).toContain('—')
    wrapper.unmount()
  })

  it('çok uzun adresli işletmede satır taşmadan render edilir, adres artık hücrede görünmez', async () => {
    // Arrange & Act
    const wrapper = await mountView([
      companyFixture({
        addressText:
          'KALEKİM AŞ. KARADUVAR MAH. SERBEST BÖLGE 14.CADDE NO:13 AKDENİZ/MERSİN çok uzun bir adres metni burada devam ediyor',
      }),
    ])

    // Assert
    expect(wrapper.find('td').exists()).toBe(true)
    expect(wrapper.text()).not.toContain('SERBEST BÖLGE 14.CADDE')
    wrapper.unmount()
  })
})

describe('CompaniesView arama', () => {
  it('adres metnine göre arama yapılabilir', async () => {
    // Arrange
    const wrapper = await mountView([
      companyFixture({ id: 1, name: 'Firma A', addressText: 'Mersin Serbest Bölge' }),
      companyFixture({ id: 2, name: 'Firma B', addressText: 'Adana Sanayi Sitesi' }),
    ])

    // Act
    await wrapper.get('input[type="text"]').setValue('Mersin')
    await flushPromises()

    // Assert
    const bodyRows = wrapper.findAll('tbody tr')
    expect(bodyRows).toHaveLength(1)
    expect(bodyRows[0].text()).toContain('Firma A')
    wrapper.unmount()
  })

  it('yetkili adına göre arama yapılabilir', async () => {
    // Arrange
    const wrapper = await mountView([
      companyFixture({ id: 1, name: 'Firma A', contactFirstName: 'Ayşe', contactLastName: 'Kaya' }),
      companyFixture({ id: 2, name: 'Firma B', contactFirstName: 'Zeynep', contactLastName: 'Şahin' }),
    ])

    // Act
    await wrapper.get('input[type="text"]').setValue('Şahin')
    await flushPromises()

    // Assert
    const bodyRows = wrapper.findAll('tbody tr')
    expect(bodyRows).toHaveLength(1)
    expect(bodyRows[0].text()).toContain('Firma B')
    wrapper.unmount()
  })

  it('arama kutusunun yer tutucusu yetkiliyi de kapsar', async () => {
    // Arrange & Act
    const wrapper = await mountView([companyFixture()])

    // Assert
    expect(wrapper.get('input[type="text"]').attributes('placeholder')).toBe(labels.company.searchPlaceholder)
    wrapper.unmount()
  })
})
