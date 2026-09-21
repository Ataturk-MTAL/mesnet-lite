import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import ImportExportView from './ImportExportView.vue'
import { labels } from '../i18n/labels'
import type { StudentListPreview, StudentListSummary } from '../api/studentListImport'
import type { TermWithDates } from '../types/models'

vi.mock('../composables/useTerm', () => ({
  activeTerm: ref('2026-2027/1'),
}))

// Tauri çalışma zamanı testte yoktur; komut adları ve argümanlar burada gözlenir.
const callMock = vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>()
vi.mock('../api/client', () => ({
  call: (command: string, args?: Record<string, unknown>) => callMock(command, args),
}))

// Toast bileşeni görünümde değil App'te durur; bildirim çağrısı doğrudan gözlenir.
const toastAddMock = vi.fn<(message: { severity: string; detail?: string }) => void>()
vi.mock('openvue/usetoast', () => ({
  useToast: () => ({ add: toastAddMock }),
}))

function mountView() {
  return mount(ImportExportView, {
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }], ToastService],
    },
    attachTo: document.body,
  })
}

function clickButton(wrapper: ReturnType<typeof mountView>, testId: string): void {
  const button = wrapper.find(`[data-testid="${testId}"]`).element as HTMLButtonElement
  button.dispatchEvent(new MouseEvent('click', { bubbles: true }))
}

beforeEach(() => {
  callMock.mockReset()
  toastAddMock.mockReset()
  document.body.innerHTML = ''
})

describe('ImportExportView commission minutes', () => {
  it('lists the commission minutes buttons before the other reports', () => {
    const wrapper = mountView()

    const reportButtons = wrapper.findAll('.report-row button').map((b) => b.text())

    expect(reportButtons.slice(0, 2)).toEqual([
      labels.reports.commissionMinutesPdf,
      labels.reports.commissionMinutesXlsx,
    ])
    expect(reportButtons[2]).toBe(labels.reports.assignmentSheet)

    wrapper.unmount()
  })

  it('exports the PDF through export_commission_minutes_pdf and saves it', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'export_commission_minutes_pdf' ? [1, 2, 3] : '/Downloads/dosya.pdf',
    )
    const wrapper = mountView()

    clickButton(wrapper, 'commission-minutes-pdf-button')
    await vi.waitFor(() => expect(callMock).toHaveBeenCalledTimes(2))

    expect(callMock.mock.calls[0][0]).toBe('export_commission_minutes_pdf')
    expect(callMock.mock.calls[1][0]).toBe('save_to_downloads')
    const saveArgs = callMock.mock.calls[1][1] as { fileName: string; bytes: number[] }
    expect(saveArgs.fileName).toBe('Isletme-Belirleme-Komisyon-Tutanagi-2026-2027-1.pdf')
    expect(saveArgs.bytes).toEqual([1, 2, 3])

    wrapper.unmount()
  })

  it('exports the workbook through export_commission_minutes_xlsx and saves it', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'export_commission_minutes_xlsx' ? [9] : '/Downloads/dosya.xlsx',
    )
    const wrapper = mountView()

    clickButton(wrapper, 'commission-minutes-xlsx-button')
    await vi.waitFor(() => expect(callMock).toHaveBeenCalledTimes(2))

    expect(callMock.mock.calls[0][0]).toBe('export_commission_minutes_xlsx')
    const saveArgs = callMock.mock.calls[1][1] as { fileName: string }
    expect(saveArgs.fileName).toBe('Isletme-Belirleme-Komisyon-Tutanagi-2026-2027-1.xlsx')

    wrapper.unmount()
  })

  it('shows the Rust error message and does not save when export fails', async () => {
    callMock.mockRejectedValue(new Error('Komisyon üyesi eksik'))
    const wrapper = mountView()

    clickButton(wrapper, 'commission-minutes-pdf-button')
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: 'Komisyon üyesi eksik' }),
    )

    expect(callMock).toHaveBeenCalledTimes(1)
    expect(callMock.mock.calls[0][0]).toBe('export_commission_minutes_pdf')

    wrapper.unmount()
  })
})

// ---------------------------------------------------------------------------
// e-Okul sınıf listesi (.XLS) içe aktarma kartı — CSV akışından bağımsızdır.
// ---------------------------------------------------------------------------

const planningTerm: TermWithDates = {
  term: '2026-2027/1',
  startDate: '2026-09-01',
  endDate: '2027-06-30',
  datesConfirmed: true,
  isPlanning: true,
  defaultAsOf: '2026-09-01',
  earliestAllowedDate: '2026-09-01',
}

function studentListPreviewFixture(): StudentListPreview {
  return {
    classes: [
      {
        fileName: '12-C.xls',
        grade: '12',
        fieldName: 'Elektrik-Elektronik Teknolojisi',
        newCount: 1,
        changedCount: 1,
        unchangedCount: 1,
        removedCount: 0,
        rows: [
          {
            studentNo: '101',
            firstName: 'Ali',
            lastName: 'Veli',
            branch: 'Elektrik Tesisatları',
            status: 'new',
            previous: null,
          },
          {
            studentNo: '102',
            firstName: 'Ayşe',
            lastName: 'Kara',
            branch: 'Elektronik Haberleşme',
            status: 'changed',
            previous: { firstName: 'Ayşe', lastName: 'Kara', grade: '11', branch: 'Elektrik Tesisatları' },
          },
          {
            studentNo: '103',
            firstName: 'Mehmet',
            lastName: 'Demir',
            branch: 'Elektrik Tesisatları',
            status: 'unchanged',
            previous: null,
          },
        ],
      },
    ],
    warnings: ['12-D.xls dosyasında 1 satır ayrıştırılamadı.'],
  }
}

function studentListSummaryFixture(): StudentListSummary {
  return { created: 1, updated: 1, skipped: 0, removed: 0, warnings: [] }
}

/** Gerçek `FileList`i taklit eden dizi benzeri nesne; `Array.from` bunu düz bir diziye çevirebilir. */
function createFileList(files: File[]): FileList {
  const list = files.slice() as unknown as FileList & { item: (index: number) => File | null }
  Object.defineProperty(list, 'item', { value: (index: number) => files[index] ?? null })
  return list
}

function setStudentListFiles(wrapper: ReturnType<typeof mountView>, files: File[]): void {
  const input = wrapper.find('[data-testid="student-list-file-input"]').element as HTMLInputElement
  Object.defineProperty(input, 'files', { value: createFileList(files), configurable: true })
  input.dispatchEvent(new Event('change', { bubbles: true }))
}

async function selectStudentListFile(wrapper: ReturnType<typeof mountView>, name = '12-C.xls'): Promise<void> {
  const file = new File([new Uint8Array([1, 2, 3])], name, { type: 'application/vnd.ms-excel' })
  setStudentListFiles(wrapper, [file])
  await flushPromises()
}

describe('ImportExportView e-Okul sınıf listesi içe aktarma', () => {
  it('reads the selected file as raw bytes and calls preview_student_list_import with them', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'preview_student_list_import' ? studentListPreviewFixture() : undefined,
    )
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() => expect(callMock).toHaveBeenCalledTimes(1))

    expect(callMock.mock.calls[0][0]).toBe('preview_student_list_import')
    const args = callMock.mock.calls[0][1] as { files: Array<{ name: string; content: number[] }> }
    expect(args.files).toEqual([{ name: '12-C.xls', content: [1, 2, 3] }])

    await flushPromises()
    expect(wrapper.text()).toContain('12-C.xls')
    expect(wrapper.text()).toContain('Elektrik-Elektronik Teknolojisi')

    wrapper.unmount()
  })

  it('shows new/changed/unchanged rows with the previous-value hint and the preview warnings', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'preview_student_list_import' ? studentListPreviewFixture() : undefined,
    )
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() => expect(wrapper.text()).toContain('Ali'))

    expect(wrapper.text()).toContain(labels.studentListImport.statusNew)
    expect(wrapper.text()).toContain(labels.studentListImport.statusChanged)
    expect(wrapper.text()).toContain(labels.studentListImport.statusUnchanged)
    expect(wrapper.text()).toContain(
      labels.studentListImport.previousValue('Ayşe', 'Kara', '11', 'Elektrik Tesisatları'),
    )
    expect(wrapper.text()).toContain('12-D.xls dosyasında 1 satır ayrıştırılamadı.')

    wrapper.unmount()
  })

  it('removes a selected file from the chip list and invalidates the stale preview', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'preview_student_list_import' ? studentListPreviewFixture() : undefined,
    )
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() => expect(wrapper.text()).toContain('12-C.xls'))

    await wrapper.findComponent({ name: 'Chip' }).vm.$emit('remove', new Event('click'))
    await flushPromises()

    expect(wrapper.find('[data-testid="student-list-preview-button"]').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).not.toContain(labels.studentListImport.previewTitle)

    wrapper.unmount()
  })

  it('reports a file that failed to read without discarding the ones that succeeded', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'preview_student_list_import' ? studentListPreviewFixture() : undefined,
    )
    const wrapper = mountView()

    const goodFile = new File([new Uint8Array([1])], 'good.xls', { type: 'application/vnd.ms-excel' })
    const badFile = new File([new Uint8Array([2])], 'bad.xls', { type: 'application/vnd.ms-excel' })
    Object.defineProperty(badFile, 'arrayBuffer', { value: () => Promise.reject(new Error('okunamadı')) })

    setStudentListFiles(wrapper, [goodFile, badFile])
    await flushPromises()

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'error', detail: labels.studentListImport.fileReadError('bad.xls') }),
    )
    // `good.xls` başarıyla okundu; önizleme düğmesi hâlâ etkin olmalı.
    expect(wrapper.find('[data-testid="student-list-preview-button"]').attributes('disabled')).toBeUndefined()

    wrapper.unmount()
  })

  it('opens the change-details dialog and applies with the chosen reason, then shows the Rust summary', async () => {
    callMock.mockImplementation(async (command) => {
      if (command === 'preview_student_list_import') return studentListPreviewFixture()
      if (command === 'list_terms_with_dates') return [planningTerm]
      if (command === 'apply_student_list_import') return studentListSummaryFixture()
      return undefined
    })
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    // Dosya adı seçim anında zaten chip olarak görünür; önizleme kartının
    // gerçekten geldiğini anlamak için kartın başlığını bekle.
    await vi.waitFor(() => expect(wrapper.text()).toContain(labels.studentListImport.previewTitle))

    clickButton(wrapper, 'student-list-apply-button')
    await vi.waitFor(() =>
      expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull(),
    )

    const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
    reasonField.value = 'e-Okul listesi güncellemesi'
    reasonField.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()

    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() =>
      expect(callMock).toHaveBeenCalledWith('apply_student_list_import', expect.anything()),
    )

    const applyArgs = callMock.mock.calls.find((call) => call[0] === 'apply_student_list_import')![1] as {
      files: Array<{ name: string; content: number[] }>
      effectiveDate: string | null
      reason: string
    }
    expect(applyArgs.files).toEqual([{ name: '12-C.xls', content: [1, 2, 3] }])
    expect(applyArgs.effectiveDate).toBeNull()
    expect(applyArgs.reason).toBe('e-Okul listesi güncellemesi')

    await flushPromises()
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'success', summary: labels.studentListImport.applied }),
    )
    expect(wrapper.text()).toContain(labels.importCsv.resultCreated)

    wrapper.unmount()
  })

  it('shows the Rust error message as-is when the import fails, without swallowing it', async () => {
    callMock.mockImplementation(async (command) => {
      if (command === 'preview_student_list_import') return studentListPreviewFixture()
      if (command === 'list_terms_with_dates') return [planningTerm]
      if (command === 'apply_student_list_import') throw new Error('12-C.xls dosyasında sınıf alanı okunamadı')
      return undefined
    })
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() => expect(wrapper.text()).toContain(labels.studentListImport.previewTitle))

    clickButton(wrapper, 'student-list-apply-button')
    await vi.waitFor(() =>
      expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull(),
    )

    const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
    reasonField.value = 'gerekçe'
    reasonField.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()

    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() =>
      expect(toastAddMock).toHaveBeenCalledWith(
        expect.objectContaining({
          severity: 'error',
          detail: expect.stringContaining('12-C.xls dosyasında sınıf alanı okunamadı'),
        }),
      ),
    )

    wrapper.unmount()
  })
})

// ---------------------------------------------------------------------------
// e-Okul sınıf listesi — içe aktarmada listede olmayan öğrencinin silinmesi.
// ---------------------------------------------------------------------------

function studentListPreviewWithRemovedFixture(): StudentListPreview {
  return {
    classes: [
      {
        fileName: '12-C.xls',
        grade: '12',
        fieldName: 'Elektrik-Elektronik Teknolojisi',
        newCount: 0,
        changedCount: 0,
        unchangedCount: 0,
        removedCount: 1,
        rows: [
          {
            // `removed` satırda dosyadan gelen üst alanlar boştur; kimlik
            // yalnız `previous` içindedir.
            studentNo: null,
            firstName: '',
            lastName: '',
            branch: '',
            status: 'removed',
            previous: { firstName: 'Zeynep', lastName: 'Yıldız', grade: '12', branch: 'Elektrik Tesisatları' },
          },
        ],
      },
    ],
    warnings: [],
  }
}

describe('ImportExportView e-Okul sınıf listesi — silinecek öğrenciler', () => {
  it('reads the removed row identity from previous, not from the empty top-level fields', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'preview_student_list_import' ? studentListPreviewWithRemovedFixture() : undefined,
    )
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() => expect(wrapper.text()).toContain('Zeynep'))

    expect(wrapper.text()).toContain('Zeynep Yıldız')
    expect(wrapper.text()).toContain(labels.studentListImport.statusRemoved)

    wrapper.unmount()
  })

  it('shows a visible destructive warning with the removed count before the user can confirm', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'preview_student_list_import' ? studentListPreviewWithRemovedFixture() : undefined,
    )
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() =>
      expect(wrapper.find('[data-testid="student-list-removal-warning"]').exists()).toBe(true),
    )

    expect(wrapper.text()).toContain(labels.studentListImport.removalWarning(1))
    // Sınıf başlığında da sayaç görünmeli.
    expect(wrapper.text()).toContain(`1 ${labels.studentListImport.statusRemoved}`)

    wrapper.unmount()
  })

  it('does not show the destructive warning when nothing will be removed', async () => {
    callMock.mockImplementation(async (command) =>
      command === 'preview_student_list_import' ? studentListPreviewFixture() : undefined,
    )
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() => expect(wrapper.text()).toContain(labels.studentListImport.previewTitle))

    expect(wrapper.find('[data-testid="student-list-removal-warning"]').exists()).toBe(false)

    wrapper.unmount()
  })

  it('includes the removed count in the result summary after applying', async () => {
    callMock.mockImplementation(async (command) => {
      if (command === 'preview_student_list_import') return studentListPreviewWithRemovedFixture()
      if (command === 'list_terms_with_dates') return [planningTerm]
      if (command === 'apply_student_list_import') return { created: 0, updated: 0, skipped: 0, removed: 1, warnings: [] }
      return undefined
    })
    const wrapper = mountView()

    await selectStudentListFile(wrapper)
    clickButton(wrapper, 'student-list-preview-button')
    await vi.waitFor(() => expect(wrapper.text()).toContain(labels.studentListImport.previewTitle))

    clickButton(wrapper, 'student-list-apply-button')
    await vi.waitFor(() =>
      expect(document.body.querySelector('[data-testid="change-details-dialog"]')).not.toBeNull(),
    )

    const reasonField = document.body.querySelector<HTMLTextAreaElement>('[data-testid="change-details-reason"]')!
    reasonField.value = 'e-Okul listesi güncellemesi'
    reasonField.dispatchEvent(new Event('input', { bubbles: true }))
    await flushPromises()

    document.body
      .querySelector<HTMLButtonElement>('[data-testid="change-details-confirm-button"]')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true }))

    await vi.waitFor(() => expect(wrapper.text()).toContain(`1 ${labels.studentListImport.resultRemoved}`))

    wrapper.unmount()
  })
})
