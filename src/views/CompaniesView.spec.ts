import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import CompaniesView from './CompaniesView.vue'
import { labels } from '../i18n/labels'
import type { Company, CompanyRemoval, TermWithDates } from '../types/models'
import type { SettingsMap } from '../api/settings'
import type { GeocodeSummary } from '../api/files'
import type { CompanyMergeSummary } from '../api/companies'

const listMock = vi.fn<() => Promise<Company[]>>()
const removeMock = vi.fn<(id: number) => Promise<CompanyRemoval>>()
const previewMergeMock = vi.fn<(fromCompanyId: number, intoCompanyId: number) => Promise<unknown>>()
const applyMergeMock = vi.fn<(input: unknown) => Promise<CompanyMergeSummary>>()
vi.mock('../api/companies', () => ({
  companiesApi: {
    list: () => listMock(),
    get: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    remove: (id: number) => removeMock(id),
    setLocation: vi.fn(),
    previewMerge: (fromCompanyId: number, intoCompanyId: number) => previewMergeMock(fromCompanyId, intoCompanyId),
    applyMerge: (input: unknown) => applyMergeMock(input),
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

const listTermsWithDatesMock = vi.fn<() => Promise<TermWithDates[]>>()
vi.mock('../api/terms', () => ({
  listTermsWithDates: () => listTermsWithDatesMock(),
}))

vi.mock('../composables/useTerm', () => ({
  activeTerm: ref('2026-2027/1'),
}))

// CompaniesView `<Toast />`'u kendi içinde barındırmaz (App.vue'da yaşar); Rust
// hatasının olduğu gibi iletildiğini DOM yerine bu casusla doğrularız.
const toastAddMock = vi.fn<(message: { severity: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

// `<ConfirmDialog />` da App.vue'da yaşar; gerçek diyaloğu çizmek yerine
// `require`e verilen `accept` geri çağrısını yakalayıp elle tetikleriz.
const confirmRequireMock = vi.fn<(options: { accept?: () => void }) => void>()
vi.mock('openvue/useconfirm', () => ({
  useConfirm: () => ({ require: confirmRequireMock, close: vi.fn() }),
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

const startedTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-14',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: false,
  defaultAsOf: '2026-09-19',
  earliestAllowedDate: '2026-10-05',
}

async function mountView(companies: Company[]): Promise<VueWrapper> {
  listMock.mockResolvedValue(companies)
  settingsGetMock.mockResolvedValue({})
  listTermsWithDatesMock.mockResolvedValue([startedTerm])
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
  removeMock.mockReset()
  settingsGetMock.mockReset()
  listTermsWithDatesMock.mockReset()
  previewMergeMock.mockReset()
  applyMergeMock.mockReset()
  toastAddMock.mockReset()
  confirmRequireMock.mockReset()
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

describe('CompaniesView — işletme birleştirme', () => {
  const duplicateA = companyFixture({ id: 5, name: 'örnek Mekatronik Havacılık Sanayi A.Ş.' })
  const duplicateB = companyFixture({ id: 6, name: 'ÖRNEK MEKATRONİK HAVACILIK' })

  async function openMergeDialogForFirstRow(): Promise<VueWrapper> {
    const wrapper = await mountView([duplicateA, duplicateB])
    document.body
      .querySelectorAll<HTMLButtonElement>(`button[aria-label="${labels.company.merge}"]`)[0]
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    return wrapper
  }

  it('satırdaki Birleştir düğmesi o satırı kaynak alan pencereyi açar', async () => {
    // Arrange & Act
    const wrapper = await openMergeDialogForFirstRow()

    // Assert
    expect(document.body.querySelector('[data-testid="company-merge-dialog"]')).not.toBeNull()
    expect(document.body.textContent).toContain(duplicateA.name)
    wrapper.unmount()
  })

  it('uygulama isteği tarih ve gerekçeyle apply_company_merge’e gider, liste yenilenir', async () => {
    // Arrange
    previewMergeMock.mockResolvedValue({
      fromName: duplicateA.name,
      intoName: duplicateB.name,
      students: [],
      awardedHoursToClear: 0,
      endsCoordination: false,
      warnings: [],
    })
    applyMergeMock.mockResolvedValue({
      movedStudents: 3,
      clearedHours: 4,
      endedCoordination: false,
      warnings: [],
    })
    const wrapper = await openMergeDialogForFirstRow()

    // Act
    wrapper.findComponent({ name: 'Select' }).vm.$emit('update:modelValue', duplicateB.id)
    await flushPromises()
    document.body
      .querySelector<HTMLButtonElement>('[data-testid="company-merge-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    wrapper.findComponent({ name: 'EffectiveDateField' }).vm.$emit('update:modelValue', '2026-10-15')
    const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
    reasonField.value = 'CSV tekrar kaydı birleştirildi'
    reasonField.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()
    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    // Assert
    expect(applyMergeMock).toHaveBeenCalledWith({
      fromCompanyId: duplicateA.id,
      intoCompanyId: duplicateB.id,
      effectiveDate: '2026-10-15',
      reason: 'CSV tekrar kaydı birleştirildi',
    })
    expect(listMock).toHaveBeenCalledTimes(2) // ilk yükleme + birleşme sonrası yenileme
    wrapper.unmount()
  })

  it('apply_company_merge Rust hatası döndürünce mesajı olduğu gibi gösterir, listeyi yenilemez', async () => {
    // Arrange
    previewMergeMock.mockResolvedValue({
      fromName: duplicateA.name,
      intoName: duplicateB.name,
      students: [],
      awardedHoursToClear: 0,
      endsCoordination: false,
      warnings: [],
    })
    applyMergeMock.mockRejectedValue(new Error('apply_company_merge: Kaynağın devam eden bir ataması var'))
    const wrapper = await openMergeDialogForFirstRow()

    // Act
    wrapper.findComponent({ name: 'Select' }).vm.$emit('update:modelValue', duplicateB.id)
    await flushPromises()
    document.body
      .querySelector<HTMLButtonElement>('[data-testid="company-merge-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    wrapper.findComponent({ name: 'EffectiveDateField' }).vm.$emit('update:modelValue', '2026-10-15')
    const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
    reasonField.value = 'gerekçe'
    reasonField.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()
    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    // Assert
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: 'error',
        detail: 'apply_company_merge: Kaynağın devam eden bir ataması var',
      }),
    )
    expect(listMock).toHaveBeenCalledTimes(1) // yalnız ilk yükleme; hata sonrası yenilenmedi
    wrapper.unmount()
  })
})

describe('CompaniesView — işletme silme', () => {
  async function clickDeleteAndAccept(): Promise<void> {
    document.body
      .querySelector<HTMLButtonElement>(`button[aria-label="${labels.common.delete}"]`)!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    // `<ConfirmDialog />` App.vue'da yaşadığı için burada çizilmez; onay
    // penceresinin "Evet" düğmesine basılmışçasına `accept` geri çağrısını tetikleriz.
    const calls = confirmRequireMock.mock.calls
    const options = calls[calls.length - 1]?.[0]
    options?.accept?.()
    await flushPromises()
  }

  it('softDeleted false dönünce mevcut "silindi" mesajını gösterir', async () => {
    // Arrange
    removeMock.mockResolvedValue({ softDeleted: false })
    const wrapper = await mountView([companyFixture()])

    // Act
    await clickDeleteAndAccept()

    // Assert
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'success', summary: labels.common.deleted }),
    )
    expect(listMock).toHaveBeenCalledTimes(2) // ilk yükleme + silme sonrası yenileme
    wrapper.unmount()
  })

  it('softDeleted true dönünce pasife alma mesajını gösterir, hata değildir', async () => {
    // Arrange
    removeMock.mockResolvedValue({ softDeleted: true })
    const wrapper = await mountView([companyFixture()])

    // Act
    await clickDeleteAndAccept()

    // Assert
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: 'info',
        summary: labels.company.deletedSoftSummary,
        detail: labels.company.deletedSoftDetail,
      }),
    )
    expect(listMock).toHaveBeenCalledTimes(2) // ilk yükleme + silme sonrası yenileme
    wrapper.unmount()
  })

  it('Rust hatası döndürünce mesajı olduğu gibi gösterir, listeyi yenilemez', async () => {
    // Arrange
    removeMock.mockRejectedValue(new Error('delete_company: açık yerleştirmesi var'))
    const wrapper = await mountView([companyFixture()])

    // Act
    await clickDeleteAndAccept()

    // Assert
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'delete_company: açık yerleştirmesi var' }),
    )
    expect(listMock).toHaveBeenCalledTimes(1) // yalnız ilk yükleme; hata sonrası yenilenmedi
    wrapper.unmount()
  })
})
