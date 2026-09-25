import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAsOfDateStore } from './asOfDate'
import type { TermWithDates } from '../types/models'

describe('useAsOfDateStore', () => {
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

    const store = useAsOfDateStore()
    store.initializeFromTerm(planningTerm)

    expect(store.asOfDate).toBe('2026-11-15')
    expect(store.isToday).toBe(false)
    expect(store.isReadOnly).toBe(true)
  })
})
