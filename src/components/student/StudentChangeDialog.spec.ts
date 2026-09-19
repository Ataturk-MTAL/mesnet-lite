import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import ToastService from 'openvue/toastservice'
import CompanyFormDialog from '../company/CompanyFormDialog.vue'
import StudentChangeDialog from './StudentChangeDialog.vue'
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
    addressText: 'Adres',
    latitude: null,
    longitude: null,
    geocodeStatus: 'pending',
    oneWayDistanceKm: null,
    notes: '',
    createdAt: '2026-01-01',
    updatedAt: '2026-01-01',
  },
]

function mountDialog(term: TermWithDates) {
  return mount(StudentChangeDialog, {
    props: {
      visible: true,
      student: { id: 42, fullName: 'Ayşe Kaya', companyId: 2, companyName: 'Eski İşletme' },
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

  it('requires a reason', async () => {
    const wrapper = mountDialog(planningTerm)
    await nextTick()

    await wrapper.getComponent({ ref: 'companySelect' }).vm.$emit('update:modelValue', 5)
    await nextTick()

    expect(document.body.textContent ?? '').toContain(labels.history.reasonRequired)
    expect(submitButton().disabled).toBe(true)

    click(submitButton())
    expect(previewChangeMock).not.toHaveBeenCalled()

    wrapper.unmount()
  })
})
