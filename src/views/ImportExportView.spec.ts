import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import Aura from '@openvue/themes/aura'
import ImportExportView from './ImportExportView.vue'
import { labels } from '../i18n/labels'

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
