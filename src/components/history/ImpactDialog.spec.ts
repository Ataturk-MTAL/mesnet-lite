import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import OpenVue from 'openvue/config'
import Aura from '@openvue/themes/aura'
import ImpactDialog from './ImpactDialog.vue'
import { useChange } from '../../composables/useChange'
import { labels } from '../../i18n/labels'
import type { ChangeOutcome, ChangeRequest, ImpactSummary } from '../../types/models'

// `useChange` gerçek haliyle kullanılır; yalnız Tauri komutlarına inen
// `api/history.ts` sarmalayıcısı taklit edilir (bkz. useChange.spec.ts).
const previewChangeMock = vi.fn<(request: ChangeRequest) => Promise<ChangeOutcome>>()
const commitChangeMock = vi.fn<(request: ChangeRequest, expectedHighWater: number | null) => Promise<ChangeOutcome>>()

vi.mock('../../api/history', () => ({
  previewChange: (request: ChangeRequest) => previewChangeMock(request),
  commitChange: (request: ChangeRequest, expectedHighWater: number | null) =>
    commitChangeMock(request, expectedHighWater),
  listHistory: vi.fn(),
  getSubjectHistory: vi.fn(),
}))

function baseRequest(): ChangeRequest {
  return {
    term: '2026-2027/1',
    effectiveDate: '2026-10-10',
    documentDate: null,
    reason: 'test gerekçesi',
    command: { type: 'studentLeaves', studentId: 1, fromCompanyId: 2 },
  }
}

const impactFixture: ImpactSummary = {
  effectiveDate: '2026-10-10',
  isPlanning: false,
  shadowedUntil: null,
  primary: [],
  automatic: [],
  warnings: [],
  notices: [],
}

// Dialog, içeriğini `document.body`'ye teleport eder; VueWrapper'ın kendi
// ağacı yalnız teleport yer tutucusunu görür. Bu yüzden içerik doğrulaması
// `document.body` üzerinden yapılır (bkz. Vue Test Utils Teleport rehberi).
function mountDialog(change: ReturnType<typeof useChange>) {
  return mount(ImpactDialog, {
    props: { change },
    global: {
      plugins: [[OpenVue, { theme: { preset: Aura, options: { darkModeSelector: '.app-dark' } } }]],
    },
    attachTo: document.body,
  })
}

function getByTestId(testId: string): HTMLElement {
  const element = document.body.querySelector<HTMLElement>(`[data-testid="${testId}"]`)
  if (!element) throw new Error(`data-testid="${testId}" bulunamadı`)
  return element
}

beforeEach(() => {
  previewChangeMock.mockReset()
  commitChangeMock.mockReset()
  document.body.innerHTML = ''
})

describe('ImpactDialog', () => {
  it('renders four sections and disables commit on rejected', async () => {
    previewChangeMock.mockResolvedValueOnce({
      status: 'rejected',
      code: 'outOfTerm',
      reason: 'Tarih dönem aralığının dışında.',
      conflictingChangeSetIds: [],
      suggestedDate: null,
    })

    const change = useChange()
    await change.preview(baseRequest())
    expect(change.status.value).toBe('rejected')

    const wrapper = mountDialog(change)
    await nextTick()
    const text = document.body.textContent ?? ''

    expect(text).toContain(labels.impact.primarySection)
    expect(text).toContain(labels.impact.automaticSection)
    expect(text).toContain(labels.impact.warningsSection)
    expect(text).toContain(labels.impact.noticesSection)
    expect(text).toContain(labels.history.rejectionCode.outOfTerm)

    const confirmButton = getByTestId('impact-confirm-button') as HTMLButtonElement
    expect(confirmButton.disabled).toBe(true)

    wrapper.unmount()
  })

  it('offers suggested date on previousMonthClosed', async () => {
    previewChangeMock
      .mockResolvedValueOnce({
        status: 'rejected',
        code: 'previousMonthClosed',
        reason: 'Ekim puantajı ilçeye gönderildi; en erken 2026-11-01 ile girebilirsiniz.',
        conflictingChangeSetIds: [],
        suggestedDate: '2026-11-01',
      })
      .mockResolvedValueOnce({ status: 'preview', impact: impactFixture, highWater: 3 })

    const change = useChange()
    await change.preview(baseRequest())
    expect(change.canUseSuggestedDate.value).toBe(true)

    const wrapper = mountDialog(change)
    await nextTick()
    const suggestedButton = getByTestId('impact-suggested-date-button')
    expect(suggestedButton.textContent).toBe(labels.history.useSuggestedDate)

    suggestedButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => expect(change.status.value).toBe('previewed'))

    expect(previewChangeMock).toHaveBeenCalledTimes(2)
    const secondCallRequest = previewChangeMock.mock.calls[1][0]
    expect(secondCallRequest.effectiveDate).toBe('2026-11-01')

    wrapper.unmount()
  })
})
