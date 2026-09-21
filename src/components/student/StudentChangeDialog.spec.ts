import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import ToastService from 'openvue/toastservice'
import CompanyFormDialog from '../company/CompanyFormDialog.vue'
import StudentChangeDialog, { type StudentChangeSubject } from './StudentChangeDialog.vue'
import { labels } from '../../i18n/labels'
import type { ChangeOutcome, ChangeRequest, Company, ImpactSummary, NewCompany, TermWithDates } from '../../types/models'

// `useChange` gerçek haliyle kullanılır; yalnız Tauri komutlarına inen
// `api/history.ts` ve `api/companies.ts` sarmalayıcıları taklit edilir
// (bkz. ImpactDialog.spec.ts'deki desen).
const previewChangeMock = vi.fn<(request: ChangeRequest) => Promise<ChangeOutcome>>()
const commitChangeMock = vi.fn<(request: ChangeRequest, expectedHighWater: number | null) => Promise<ChangeOutcome>>()
const listCompaniesMock = vi.fn<() => Promise<Company[]>>()
const createCompanyMock = vi.fn<(input: NewCompany) => Promise<Company>>()

vi.mock('../../api/history', () => ({
  previewChange: (request: ChangeRequest) => previewChangeMock(request),
  commitChange: (request: ChangeRequest, expectedHighWater: number | null) =>
    commitChangeMock(request, expectedHighWater),
  listHistory: vi.fn(),
  getSubjectHistory: vi.fn(),
}))

vi.mock('../../api/companies', () => ({
  companiesApi: {
    list: () => listCompaniesMock(),
    get: vi.fn(),
    create: (input: NewCompany) => createCompanyMock(input),
    update: vi.fn(),
    remove: vi.fn(),
    setLocation: vi.fn(),
  },
}))

const impactFixture: ImpactSummary = {
  effectiveDate: '2026-10-10',
  isPlanning: true,
  shadowedUntil: null,
  primary: [],
  automatic: [],
  warnings: [],
  notices: [],
}

const planningTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-14',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: true,
  defaultAsOf: '2026-09-19',
  earliestAllowedDate: '2026-09-14',
}

const startedTerm: TermWithDates = { ...planningTerm, isPlanning: false }

const companiesFixture: Company[] = [
  {
    id: 5,
    name: 'Yeni Teknoloji A.Ş.',
    contactFirstName: 'Ali',
    contactLastName: 'Veli',
    phone: '',
    email: '',
    addressText: '',
    district: '',
    latitude: null,
    longitude: null,
    geocodeStatus: 'pending',
    oneWayDistanceKm: null,
    notes: '',
    createdAt: '2026-01-01',
    updatedAt: '2026-01-01',
  },
  {
    id: 6,
    name: 'Örnek Mekatronik Havacılık Sanayi A.Ş.',
    contactFirstName: 'Zeynep',
    contactLastName: 'Şahin',
    phone: '',
    email: '',
    addressText: 'Adana Sanayi Sitesi 14. Cadde No:13 Yüreğir/Adana',
    district: 'Yüreğir',
    latitude: null,
    longitude: null,
    geocodeStatus: 'pending',
    oneWayDistanceKm: 8.2,
    notes: '',
    createdAt: '2026-01-01',
    updatedAt: '2026-01-01',
  },
  {
    id: 7,
    name: 'İlçesiz Adresli İşletme',
    contactFirstName: 'Can',
    contactLastName: 'Yıldız',
    phone: '',
    email: '',
    addressText: 'Bu işletmenin ilçesi ayrıştırılamadı; yalnızca uzun bir açık adres metni girildi buraya',
    district: '',
    latitude: null,
    longitude: null,
    geocodeStatus: 'pending',
    oneWayDistanceKm: null,
    notes: '',
    createdAt: '2026-01-01',
    updatedAt: '2026-01-01',
  },
]

const transferSubject: StudentChangeSubject = { id: 42, fullName: 'Ayşe Kaya', companyId: 2, companyName: 'Eski İşletme' }
const placingSubject: StudentChangeSubject = { id: 43, fullName: 'Mehmet Can', companyId: null, companyName: null }

function mountDialog(term: TermWithDates, student: StudentChangeSubject = transferSubject) {
  return mount(StudentChangeDialog, {
    props: {
      visible: true,
      student,
      term,
    },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
}

// Dialog, içeriğini `document.body`'ye teleport eder; VueWrapper'ın kendi
// ağacı yalnız teleport yer tutucusunu görür (bkz. ImpactDialog.spec.ts).
async function fillReason(text: string): Promise<void> {
  const textarea = document.body.querySelector('textarea')
  if (!textarea) throw new Error('textarea bulunamadı')
  textarea.value = text
  textarea.dispatchEvent(new Event('input'))
  await nextTick()
}

// Hedef işletme `Select`'ini açar; seçenek satırları ancak açılınca DOM'a gelir
// (Portal ile `document.body`'ye teleport edilir).
async function openCompanySelect(): Promise<void> {
  const container = document.body.querySelector<HTMLElement>('.p-select')
  if (!container) throw new Error('işletme select bulunamadı')
  container.click()
  await nextTick()
  await nextTick()
}

function optionByLabel(companyName: string): HTMLElement {
  const option = document.body.querySelector<HTMLElement>(`[role="option"][aria-label="${companyName}"]`)
  if (!option) throw new Error(`"${companyName}" seçeneği bulunamadı`)
  return option
}

function getByTestId(testId: string): HTMLButtonElement {
  const element = document.body.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)
  if (!element) throw new Error(`data-testid="${testId}" bulunamadı`)
  return element
}

function submitButton(): HTMLButtonElement {
  return getByTestId('student-change-submit-button')
}

beforeEach(() => {
  previewChangeMock.mockReset()
  commitChangeMock.mockReset()
  listCompaniesMock.mockReset().mockResolvedValue(companiesFixture)
  createCompanyMock.mockReset()
  document.body.innerHTML = ''
})

// `HTMLElement.click()` — jsdom'un etkinleştirme davranışı, `disabled`
// düğmede olayı hiç göndermez; elle `dispatchEvent` bu korumayı atlar.
function click(button: HTMLButtonElement): void {
  button.click()
}

describe('StudentChangeDialog', () => {
  it('builds a transfer to an existing company', async () => {
    previewChangeMock.mockResolvedValueOnce({ status: 'preview', impact: impactFixture, highWater: 1 })

    const wrapper = mountDialog(planningTerm)
    await nextTick()

    await fillReason('test gerekçesi')
    await wrapper.getComponent({ ref: 'companySelect' }).vm.$emit('update:modelValue', 5)
    await nextTick()

    expect(submitButton().disabled).toBe(false)
    click(submitButton())
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    expect(previewChangeMock).toHaveBeenCalledWith({
      term: planningTerm.term,
      effectiveDate: null,
      documentDate: null,
      reason: 'test gerekçesi',
      command: { type: 'transferStudent', studentId: 42, fromCompanyId: 2, to: { type: 'existing', companyId: 5 } },
    })

    wrapper.unmount()
  })

  it('builds a transfer to a new company without saving it first', async () => {
    previewChangeMock.mockResolvedValueOnce({ status: 'preview', impact: impactFixture, highWater: 1 })

    const wrapper = mountDialog(planningTerm)
    await nextTick()

    const newCompany: NewCompany = {
      name: 'Yerinde Kurulan İşletme',
      contactFirstName: 'Can',
      contactLastName: 'Demir',
      phone: '',
      email: '',
      addressText: 'Adres',
      district: '',
      latitude: null,
      longitude: null,
      oneWayDistanceKm: null,
      notes: '',
    }

    await wrapper.getComponent({ ref: 'targetModeSelect' }).vm.$emit('update:modelValue', 'new')
    await nextTick()
    await wrapper.getComponent(CompanyFormDialog).vm.$emit('save', newCompany)
    await nextTick()

    await fillReason('test gerekçesi')
    click(submitButton())
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    expect(createCompanyMock).not.toHaveBeenCalled()
    expect(previewChangeMock).toHaveBeenCalledWith({
      term: planningTerm.term,
      effectiveDate: null,
      documentDate: null,
      reason: 'test gerekçesi',
      command: { type: 'transferStudent', studentId: 42, fromCompanyId: 2, to: { type: 'new', company: newCompany } },
    })

    wrapper.unmount()
  })

  it('builds a leave with contract end label', async () => {
    previewChangeMock.mockResolvedValueOnce({
      status: 'preview',
      impact: { ...impactFixture, isPlanning: false },
      highWater: 1,
    })

    const wrapper = mountDialog(startedTerm)
    await nextTick()

    await wrapper.getComponent({ ref: 'modeSelect' }).vm.$emit('update:modelValue', 'leave')
    await nextTick()

    const dateField = wrapper.getComponent({ ref: 'dateField' })
    expect(dateField.props('label')).toBe(labels.effectiveDateField.contractEnd)

    await dateField.vm.$emit('update:modelValue', '2026-10-10')
    await fillReason('ayrılış gerekçesi')
    await nextTick()

    expect(submitButton().disabled).toBe(false)
    click(submitButton())
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    expect(previewChangeMock).toHaveBeenCalledWith({
      term: startedTerm.term,
      effectiveDate: '2026-10-10',
      documentDate: null,
      reason: 'ayrılış gerekçesi',
      command: { type: 'studentLeaves', studentId: 42, fromCompanyId: 2 },
    })

    wrapper.unmount()
  })

  it('boş diyalog açılışta gerekçe hatasını göstermez', async () => {
    const wrapper = mountDialog(planningTerm)
    await nextTick()

    expect(document.body.textContent ?? '').not.toContain(labels.history.reasonRequired)
    expect(submitButton().disabled).toBe(true)

    wrapper.unmount()
  })

  it('gerekçe alanına dokunulup boş bırakılınca hatayı gösterir', async () => {
    const wrapper = mountDialog(planningTerm)
    await nextTick()

    await wrapper.getComponent({ ref: 'companySelect' }).vm.$emit('update:modelValue', 5)
    document.body.querySelector('textarea')!.dispatchEvent(new Event('blur'))
    await nextTick()

    expect(document.body.textContent ?? '').toContain(labels.history.reasonRequired)
    expect(submitButton().disabled).toBe(true)

    click(submitButton())
    expect(previewChangeMock).not.toHaveBeenCalled()

    wrapper.unmount()
  })
})

describe('StudentChangeDialog — ilk yerleştirme kipi', () => {
  it('işletmesi olmayan öğrenci için kip seçiciyi gizler, başlığı değiştirir', async () => {
    const wrapper = mountDialog(planningTerm, placingSubject)
    await nextTick()

    expect(document.body.textContent ?? '').toContain(labels.studentChange.placeTitle)
    expect(document.body.querySelector(`[aria-label="${labels.studentChange.title}"]`)).toBeNull()
    expect(document.body.textContent ?? '').not.toContain(labels.studentChange.fromCompany)

    wrapper.unmount()
  })

  it('mevcut işletme seçilirse placeStudent komutu üretir', async () => {
    previewChangeMock.mockResolvedValueOnce({ status: 'preview', impact: impactFixture, highWater: 1 })

    const wrapper = mountDialog(planningTerm, placingSubject)
    await nextTick()

    await fillReason('ilk yerleştirme gerekçesi')
    await wrapper.getComponent({ ref: 'companySelect' }).vm.$emit('update:modelValue', 5)
    await nextTick()

    expect(submitButton().disabled).toBe(false)
    click(submitButton())
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    expect(previewChangeMock).toHaveBeenCalledWith({
      term: planningTerm.term,
      effectiveDate: null,
      documentDate: null,
      reason: 'ilk yerleştirme gerekçesi',
      command: { type: 'placeStudent', studentId: 43, to: { type: 'existing', companyId: 5 } },
    })

    wrapper.unmount()
  })

  it('yeni işletme seçilirse placeStudent komutu new hedefiyle üretir', async () => {
    previewChangeMock.mockResolvedValueOnce({ status: 'preview', impact: impactFixture, highWater: 1 })

    const wrapper = mountDialog(planningTerm, placingSubject)
    await nextTick()

    const newCompany: NewCompany = {
      name: 'Yeni Kurulan İşletme',
      contactFirstName: 'Elif',
      contactLastName: 'Aydın',
      phone: '',
      email: '',
      addressText: 'Adres',
      district: '',
      latitude: null,
      longitude: null,
      oneWayDistanceKm: null,
      notes: '',
    }

    await wrapper.getComponent({ ref: 'targetModeSelect' }).vm.$emit('update:modelValue', 'new')
    await nextTick()
    await wrapper.getComponent(CompanyFormDialog).vm.$emit('save', newCompany)
    await nextTick()

    await fillReason('ilk yerleştirme gerekçesi')
    click(submitButton())
    await vi.waitFor(() => expect(previewChangeMock).toHaveBeenCalledTimes(1))

    expect(previewChangeMock).toHaveBeenCalledWith({
      term: planningTerm.term,
      effectiveDate: null,
      documentDate: null,
      reason: 'ilk yerleştirme gerekçesi',
      command: { type: 'placeStudent', studentId: 43, to: { type: 'new', company: newCompany } },
    })

    wrapper.unmount()
  })

  it('hedef seçilmeden kaydet düğmesi kapalı kalır', async () => {
    const wrapper = mountDialog(planningTerm, placingSubject)
    await nextTick()

    await fillReason('ilk yerleştirme gerekçesi')

    expect(submitButton().disabled).toBe(true)
    click(submitButton())
    expect(previewChangeMock).not.toHaveBeenCalled()

    wrapper.unmount()
  })
})

describe('StudentChangeDialog — hedef işletme seçeneklerinde konum', () => {
  it('ilçesi olan işletmenin seçeneğinde ilçe ve tek yön mesafe görünür', async () => {
    const wrapper = mountDialog(planningTerm)
    await nextTick()
    await openCompanySelect()

    const option = optionByLabel('Örnek Mekatronik Havacılık Sanayi A.Ş.')
    expect(option.querySelector('.company-option-secondary')?.textContent?.trim()).toBe('Yüreğir · 8,2 km')

    wrapper.unmount()
  })

  it('ilçesi boş, adresi olan işletmenin seçeneğinde kısaltılmış adres görünür', async () => {
    const wrapper = mountDialog(planningTerm)
    await nextTick()
    await openCompanySelect()

    const option = optionByLabel('İlçesiz Adresli İşletme')
    expect(option.querySelector('.company-option-secondary')?.textContent?.trim()).toBe(
      'Bu işletmenin ilçesi ayrıştırılamadı; yalnızca uzun bir…',
    )

    wrapper.unmount()
  })

  it('ilçesi ve adresi boş işletmenin seçeneğinde ikincil satır render edilmez', async () => {
    const wrapper = mountDialog(planningTerm)
    await nextTick()
    await openCompanySelect()

    const option = optionByLabel('Yeni Teknoloji A.Ş.')
    expect(option.querySelector('.company-option-secondary')).toBeNull()

    wrapper.unmount()
  })

  it('kapalı select yalnız işletme adını gösterir', async () => {
    const wrapper = mountDialog(planningTerm)
    await nextTick()

    await wrapper.getComponent({ ref: 'companySelect' }).vm.$emit('update:modelValue', 6)
    await nextTick()

    const closedLabel = document.body.querySelector('.p-select-label')
    expect(closedLabel?.textContent?.trim()).toBe('Örnek Mekatronik Havacılık Sanayi A.Ş.')

    wrapper.unmount()
  })
})
