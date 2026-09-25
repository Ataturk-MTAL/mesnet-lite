import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Tooltip from 'openvue/tooltip'
import Aura from '@openvue/themes/aura'
import StudentsView from './StudentsView.vue'
import { labels } from '../i18n/labels'
import type { Company, Student, TermWithDates } from '../types/models'
import { useSelectionStore } from '../stores/selection'
import { useTermStore } from '../stores/term'

const listStudentsMock = vi.fn<() => Promise<Student[]>>()
vi.mock('../api/students', () => ({
  studentsApi: {
    list: () => listStudentsMock(),
    listTerms: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    remove: vi.fn(),
  },
}))

const listCompaniesMock = vi.fn<() => Promise<Company[]>>()
vi.mock('../api/companies', () => ({
  companiesApi: {
    list: () => listCompaniesMock(),
    get: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    remove: vi.fn(),
    setLocation: vi.fn(),
  },
}))

const listTermsWithDatesMock = vi.fn<() => Promise<TermWithDates[]>>()
vi.mock('../api/terms', () => ({
  listTermsWithDates: () => listTermsWithDatesMock(),
}))

function studentFixture(overrides: Partial<Student> = {}): Student {
  return {
    id: 1,
    firstName: 'Ayşe',
    lastName: 'Kaya',
    studentNo: '101',
    grade: '12/A',
    branch: 'Elektrik-Elektronik',
    companyId: null,
    submittedAt: null,
    term: '2026-2027/1',
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

async function mountView(students: Student[], terms: TermWithDates[] = [startedTerm]): Promise<VueWrapper> {
  listStudentsMock.mockResolvedValue(students)
  listCompaniesMock.mockResolvedValue([])
  listTermsWithDatesMock.mockResolvedValue(terms)
  const wrapper = mount(StudentsView, {
    global: {
      plugins: [
        [OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }],
        ToastService,
        ConfirmationService,
      ],
      directives: { tooltip: Tooltip },
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  listStudentsMock.mockReset()
  listCompaniesMock.mockReset()
  listTermsWithDatesMock.mockReset()
  document.body.replaceChildren()
  useTermStore().activeTerm = '2026-2027/1'
})

describe('StudentsView — nakil/yerleştirme düğmesi', () => {
  it('işletmesi olmayan öğrenci için düğme etkin ve pi-briefcase ikonlu', async () => {
    // Arrange & Act
    const wrapper = await mountView([studentFixture({ companyId: null })])

    // Assert
    const button = document.body.querySelector<HTMLButtonElement>(
      `button[aria-label="${labels.studentChange.placeTitle}"]`,
    )
    expect(button).not.toBeNull()
    expect(button?.disabled).toBe(false)
    expect(button?.querySelector('.pi-briefcase')).not.toBeNull()
    wrapper.unmount()
  })

  it('işletmesi olan öğrenci için düğme nakil ikonu ve etiketiyle görünür', async () => {
    // Arrange & Act
    const wrapper = await mountView([studentFixture({ id: 2, companyId: 5 })])

    // Assert
    const button = document.body.querySelector<HTMLButtonElement>(
      `button[aria-label="${labels.studentChange.title}"]`,
    )
    expect(button).not.toBeNull()
    expect(button?.disabled).toBe(false)
    expect(button?.querySelector('.pi-arrow-right-arrow-left')).not.toBeNull()
    wrapper.unmount()
  })

  it('aktif dönemin tarihleri bilinmiyorsa düğme işletmesi olmayan öğrenci için de kapanır', async () => {
    // Arrange & Act — `startedTerm` listede yok, `currentTerm` bulunamıyor.
    const wrapper = await mountView([studentFixture({ companyId: null })], [])

    // Assert
    const button = document.body.querySelector<HTMLButtonElement>(
      `button[aria-label="${labels.studentChange.placeTitle}"]`,
    )
    expect(button?.disabled).toBe(true)
    wrapper.unmount()
  })

  it('işletmesi olmayan öğrencide düğmeye tıklayınca diyalog ilk yerleştirme başlığıyla açılır', async () => {
    // Arrange
    const wrapper = await mountView([studentFixture({ companyId: null })])

    // Act
    document.body
      .querySelector<HTMLButtonElement>(`button[aria-label="${labels.studentChange.placeTitle}"]`)!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    // Assert
    expect(document.body.textContent ?? '').toContain(labels.studentChange.placeTitle)
    wrapper.unmount()
  })
})

describe('StudentsView seçim kalıcılığı (Pinia store)', () => {
  it('arama metni yeniden mount edilince korunur', async () => {
    // Arrange
    const wrapper = await mountView([studentFixture()])
    await wrapper.get('input[type="text"]').setValue('Ayşe')
    await flushPromises()
    wrapper.unmount()

    // Act
    const selection = useSelectionStore()
    const wrapper2 = await mountView([studentFixture()])

    // Assert
    expect(selection.studentSearch).toBe('Ayşe')
    expect((wrapper2.get('input[type="text"]').element as HTMLInputElement).value).toBe('Ayşe')
    wrapper2.unmount()
  })
})
