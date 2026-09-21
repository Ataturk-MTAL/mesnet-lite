import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import CompanyMergeDialog from './CompanyMergeDialog.vue'
import EffectiveDateField from '../history/EffectiveDateField.vue'
import { labels } from '../../i18n/labels'
import type { Company, TermWithDates } from '../../types/models'
import type { CompanyMergePreview } from '../../api/companies'

const previewMergeMock = vi.fn<(fromCompanyId: number, intoCompanyId: number) => Promise<CompanyMergePreview>>()
vi.mock('../../api/companies', () => ({
  companiesApi: {
    previewMerge: (fromCompanyId: number, intoCompanyId: number) => previewMergeMock(fromCompanyId, intoCompanyId),
    applyMerge: vi.fn(),
  },
}))

const startedTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-14',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: false,
  defaultAsOf: '2026-09-19',
  earliestAllowedDate: '2026-10-05',
}

function companyFixture(overrides: Partial<Company> = {}): Company {
  return {
    id: 1,
    name: 'Örnek İşletme',
    contactFirstName: '',
    contactLastName: '',
    phone: '',
    email: '',
    addressText: 'Mersin',
    district: 'Akdeniz',
    latitude: null,
    longitude: null,
    geocodeStatus: 'pending',
    oneWayDistanceKm: null,
    notes: '',
    createdAt: '2026-01-01',
    updatedAt: '2026-01-01',
    ...overrides,
  }
}

const source = companyFixture({ id: 5, name: 'Örnek İşletme' })
const target = companyFixture({ id: 6, name: 'Örnek İşletme' })
const other = companyFixture({ id: 7, name: 'Farklı Firma' })

function previewFixture(overrides: Partial<CompanyMergePreview> = {}): CompanyMergePreview {
  return {
    fromName: source.name,
    intoName: target.name,
    students: [{ studentId: 11, fullName: 'Ahmet Yıldız' }],
    awardedHoursToClear: 4,
    endsCoordination: true,
    warnings: [],
    ...overrides,
  }
}

function mountDialog(options: { term?: TermWithDates | null } = {}) {
  return mount(CompanyMergeDialog, {
    props: {
      visible: true,
      source,
      companies: [source, target, other],
      term: options.term ?? startedTerm,
    },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

type MountedDialog = ReturnType<typeof mountDialog>

function selectComponent(wrapper: MountedDialog) {
  const select = wrapper.findComponent({ name: 'Select' })
  if (!select.exists()) throw new Error('Select bulunamadı')
  return select
}

async function pickTarget(wrapper: MountedDialog, targetId: number): Promise<void> {
  selectComponent(wrapper).vm.$emit('update:modelValue', targetId)
  await flushPromises()
}

async function clickConfirmButton(): Promise<void> {
  document.body
    .querySelector<HTMLButtonElement>('[data-testid="company-merge-confirm-button"]')!
    .dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await flushPromises()
}

async function fillChangeDetails(wrapper: MountedDialog, date: string, reason: string): Promise<void> {
  await wrapper.findComponent(EffectiveDateField).vm.$emit('update:modelValue', date)
  const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')
  if (!reasonField) throw new Error('change-details-reason bulunamadı')
  reasonField.value = reason
  reasonField.dispatchEvent(new Event('input', { bubbles: true }))
  await flushPromises()
}

async function clickChangeDetailsConfirm(): Promise<void> {
  document.body
    .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
    .dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await flushPromises()
}

beforeEach(() => {
  previewMergeMock.mockReset()
  document.body.innerHTML = ''
})

describe('CompanyMergeDialog — hedef listesi', () => {
  it('kaynağın kendisini hedef seçeneklerinde göstermez', async () => {
    // Arrange & Act
    const wrapper = mountDialog()
    await flushPromises()

    // Assert
    const select = selectComponent(wrapper)
    const options = select.props('options') as Company[]
    expect(options.map((c) => c.id)).toEqual([target.id, other.id])
    wrapper.unmount()
  })
})

describe('CompanyMergeDialog — önizleme', () => {
  it('hedef seçilince önizlemeyi doğru argümanlarla çağırır', async () => {
    // Arrange
    previewMergeMock.mockResolvedValue(previewFixture())
    const wrapper = mountDialog()
    await flushPromises()

    // Act
    await pickTarget(wrapper, target.id)

    // Assert
    expect(previewMergeMock).toHaveBeenCalledWith(source.id, target.id)
    expect(document.body.textContent).toContain(labels.companyMerge.direction(source.name, target.name))
    expect(document.body.textContent).toContain('Ahmet Yıldız')
    wrapper.unmount()
  })

  it('önizleme hata döndürünce Rust mesajını olduğu gibi gösterir', async () => {
    // Arrange
    previewMergeMock.mockRejectedValue(new Error('preview_company_merge: İşletmenin aktif koordinatörlüğü var'))
    const wrapper = mountDialog()
    await flushPromises()

    // Act
    await pickTarget(wrapper, target.id)

    // Assert
    const errorBox = document.body.querySelector('[data-testid="company-merge-preview-error"]')
    expect(errorBox?.textContent).toContain('İşletmenin aktif koordinatörlüğü var')
    document.body
      .querySelectorAll('[data-testid="company-merge-confirm-button"]')
      .forEach((button) => expect((button as HTMLButtonElement).disabled).toBe(true))
    wrapper.unmount()
  })
})

describe('CompanyMergeDialog — uygulama isteği', () => {
  it('Birleştir sonrası tarih ve gerekçeyle confirm olayını yayar', async () => {
    // Arrange
    previewMergeMock.mockResolvedValue(previewFixture())
    const wrapper = mountDialog()
    await flushPromises()
    await pickTarget(wrapper, target.id)

    // Act
    await clickConfirmButton()
    expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull()
    await fillChangeDetails(wrapper, '2026-10-15', 'CSV tekrar kaydı birleştirildi')
    await clickChangeDetailsConfirm()

    // Assert
    expect(wrapper.emitted('confirm')).toEqual([
      [
        {
          fromCompanyId: source.id,
          intoCompanyId: target.id,
          effectiveDate: '2026-10-15',
          reason: 'CSV tekrar kaydı birleştirildi',
        },
      ],
    ])
    wrapper.unmount()
  })

  it('önizleme yüklenmeden Birleştir düğmesi devre dışıdır', async () => {
    // Arrange & Act
    const wrapper = mountDialog()
    await flushPromises()

    // Assert
    const button = document.body.querySelector<HTMLButtonElement>('[data-testid="company-merge-confirm-button"]')
    expect(button?.disabled).toBe(true)
    wrapper.unmount()
  })
})
