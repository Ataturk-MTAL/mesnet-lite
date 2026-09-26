import { describe, expect, it } from 'vitest'
import { nextTick } from 'vue'
import { useAsOfDateStore } from './asOfDate'
import { useTermStore } from './term'
import type { TermWithDates } from '../types/models'

// Her testten önce YENİ bir Pinia örneği `src/test-setup.ts`te etkinleştirilir.

function termWithDates(overrides: Partial<TermWithDates> = {}): TermWithDates {
  return {
    term: '2026-2027/1',
    startDate: '2026-09-01',
    endDate: '2027-06-30',
    datesConfirmed: true,
    isPlanning: false,
    defaultAsOf: '2026-11-10',
    earliestAllowedDate: '2026-09-01',
    ...overrides,
  }
}

describe('useAsOfDateStore', () => {
  it('varsayılan tarihte salt okunur değildir ve istek tarihi null gider', () => {
    const termStore = useTermStore()
    termStore.activeTermDates = termWithDates()

    const store = useAsOfDateStore()

    expect(store.asOfDate).toBe('2026-11-10')
    expect(store.isReadOnly).toBe(false)
    expect(store.requestAsOf).toBeNull()
  })

  it('başka bir tarih seçilince salt okunur olur ve istek tarihi o tarihtir', () => {
    const termStore = useTermStore()
    termStore.activeTermDates = termWithDates()
    const store = useAsOfDateStore()

    store.setAsOfDate('2026-10-01')

    expect(store.isReadOnly).toBe(true)
    expect(store.requestAsOf).toBe('2026-10-01')
  })

  it('planlama döneminde varsayılan (dönem başı) tarih salt okunur DEĞİLDİR', () => {
    const termStore = useTermStore()
    // Dönem henüz başlamadı; backend gerçek bugünü dönem başlangıcına
    // sıkıştırıp `defaultAsOf` alanında göndermiş. Varsayılan tarih HER ZAMAN
    // düzenlenebilir olmalı, "bugün değilse kilitle" kuralı bunu bozardı.
    termStore.activeTermDates = termWithDates({
      isPlanning: true,
      startDate: '2026-11-15',
      defaultAsOf: '2026-11-15',
      earliestAllowedDate: '2026-11-15',
    })

    const store = useAsOfDateStore()

    expect(store.asOfDate).toBe('2026-11-15')
    expect(store.isReadOnly).toBe(false)
    expect(store.requestAsOf).toBeNull()
  })

  it('"Bugün" düğmesi varsayılan tarihe döner', () => {
    const termStore = useTermStore()
    termStore.activeTermDates = termWithDates()
    const store = useAsOfDateStore()
    store.setAsOfDate('2026-10-01')

    store.goToToday()

    expect(store.asOfDate).toBe(store.defaultAsOf)
    expect(store.isReadOnly).toBe(false)
  })

  it('dönem tarihleri gelince seçici varsayılana ayarlanır', async () => {
    const termStore = useTermStore()
    const store = useAsOfDateStore()
    expect(store.asOfDate).toBe('')

    termStore.activeTermDates = termWithDates()
    await nextTick()

    expect(store.asOfDate).toBe('2026-11-10')
  })

  it('dönem değişince seçici yeni dönemin varsayılanına döner, elle seçilmiş eski tarih korunmaz', async () => {
    const termStore = useTermStore()
    termStore.activeTermDates = termWithDates()
    const store = useAsOfDateStore()
    store.setAsOfDate('2026-10-01')

    termStore.activeTermDates = termWithDates({
      term: '2027-2028/1',
      startDate: '2027-09-01',
      endDate: '2028-06-30',
      defaultAsOf: '2027-09-01',
      earliestAllowedDate: '2027-09-01',
    })
    await nextTick()

    expect(store.asOfDate).toBe('2027-09-01')
    expect(store.isReadOnly).toBe(false)
  })
})
