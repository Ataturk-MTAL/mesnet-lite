import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useChange } from './useChange'
import type { ChangeOutcome, ChangeRequest, ImpactSummary } from '../types/models'

// Backend komutları henüz canlı değil; `api/history.ts` sarmalayıcısı taklit edilir.
// (AppSidebar.spec.ts'teki gibi doğrudan üst katman modülünü taklit etme deseni.)
const previewChangeMock = vi.fn<(request: ChangeRequest) => Promise<ChangeOutcome>>()
const commitChangeMock = vi.fn<(request: ChangeRequest, expectedHighWater: number | null) => Promise<ChangeOutcome>>()

vi.mock('../api/history', () => ({
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

beforeEach(() => {
  previewChangeMock.mockReset()
  commitChangeMock.mockReset()
})

describe('useChange', () => {
  it('re-previews on stale', async () => {
    previewChangeMock
      .mockResolvedValueOnce({ status: 'stale', message: 'Aradan başka bir değişiklik geçti' })
      .mockResolvedValueOnce({ status: 'preview', impact: impactFixture, highWater: 7 })

    const change = useChange()
    await change.preview(baseRequest())

    expect(previewChangeMock).toHaveBeenCalledTimes(2)
    expect(change.status.value).toBe('previewed')
    expect(change.highWater.value).toBe(7)
    expect(change.impact.value).toEqual(impactFixture)
  })

  it('offers first-of-month on previousMonthClosed', async () => {
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
    const request = baseRequest() // effectiveDate '2026-10-10' kullanıcının girdiği özgün tarih
    await change.preview(request)

    expect(change.status.value).toBe('rejected')
    expect(change.rejection.value?.code).toBe('previousMonthClosed')
    expect(change.canUseSuggestedDate.value).toBe(true)

    await change.useSuggestedDate()

    expect(previewChangeMock).toHaveBeenCalledTimes(2)
    const secondCallRequest = previewChangeMock.mock.calls[1][0]
    expect(secondCallRequest.effectiveDate).toBe('2026-11-01')
    expect(secondCallRequest.documentDate).toBe('2026-10-10')
    expect(change.status.value).toBe('previewed')
  })

  it('commits with the previewed highWater', async () => {
    previewChangeMock.mockResolvedValueOnce({ status: 'preview', impact: impactFixture, highWater: 12 })
    commitChangeMock.mockResolvedValueOnce({ status: 'committed', changeSetId: 99, impact: impactFixture })

    const change = useChange()
    await change.preview(baseRequest())
    await change.confirm()

    expect(commitChangeMock).toHaveBeenCalledWith(expect.objectContaining({ term: '2026-2027/1' }), 12)
    expect(change.status.value).toBe('committed')
    expect(change.changeSetId.value).toBe(99)
  })
})
