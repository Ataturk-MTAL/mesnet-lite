import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAsOfDate } from './useAsOfDate'
import type { TermWithDates } from '../types/models'

describe('useAsOfDate', () => {
  beforeEach(() => {
    // Gerçek "bugün" testten teste değişmesin diye sabitlenir (UTC 2026-11-10 → TR 2026-11-10).
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-11-10T09:00:00.000Z'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('clamps today to the term', () => {
    // Dönem henüz planlama aşamasında; backend gerçek bugünü dönem başlangıcına
    // sıkıştırıp `defaultAsOf` alanında göndermiş.
    const planningTerm: TermWithDates = {
      term: '2026-2027/1',
      startDate: '2026-11-15',
      endDate: '2027-01-31',
      datesConfirmed: true,
      isPlanning: true,
      defaultAsOf: '2026-11-15',
      earliestAllowedDate: '2026-11-15',
    }

    const { asOfDate, isToday, isReadOnly, initializeFromTerm } = useAsOfDate()
    initializeFromTerm(planningTerm)

    expect(asOfDate.value).toBe('2026-11-15')
    expect(isToday.value).toBe(false)
    expect(isReadOnly.value).toBe(true)
  })
})
