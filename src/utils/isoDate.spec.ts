import { describe, expect, it } from 'vitest'
import { dateToIso, isoToDate } from './isoDate'

describe('isoToDate', () => {
  it('ISO dizgisini yerel takvim gününe çevirir', () => {
    const date = isoToDate('2026-10-05')
    expect(date?.getFullYear()).toBe(2026)
    expect(date?.getMonth()).toBe(9)
    expect(date?.getDate()).toBe(5)
  })

  it('boş ya da biçimsiz girdide null döner', () => {
    expect(isoToDate(null)).toBeNull()
    expect(isoToDate('')).toBeNull()
    expect(isoToDate('05/10/2026')).toBeNull()
  })
})

describe('dateToIso', () => {
  it('yerel Date değerini YYYY-MM-DD dizgisine çevirir', () => {
    expect(dateToIso(new Date(2026, 9, 5))).toBe('2026-10-05')
  })

  it('Date dışındaki değerlerde null döner', () => {
    expect(dateToIso(null)).toBeNull()
    expect(dateToIso([new Date(2026, 9, 5)])).toBeNull()
  })
})
