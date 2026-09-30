import { describe, expect, it } from 'vitest'
import {
  buildTeacherRows,
  CHIEF_OPTIONS,
  principalCapWarning,
  principalLoadDefaults,
  turkishSortKeys,
} from './chiefRules'
import { labels } from '../i18n/labels'
import type { ChiefType, TeacherWithCapacity } from '../types/models'

function teacher(id: number, firstName: string, chiefType: ChiefType): TeacherWithCapacity {
  return {
    id, firstName, lastName: 'Deneme', registryNo: '', field: 'X', branches: '[]',
    employmentType: 'tenured', baseHours: 20, maxExtraHours: 20, otherExtraHours: 0,
    chiefType, isActive: 1, chiefHours: 0, statutoryCap: 40, capacity: 20,
  }
}

describe('chiefRules', () => {
  it('seçenekleri istenen sırayla verir', () => {
    expect(CHIEF_OPTIONS.map((o) => o.value)).toEqual([
      'none', 'workshop_lab', 'department', 'deputy_principal', 'principal',
    ])
  })

  it('müdür ve yardımcısı için 6 saatten fazla ek dersi uyarır', () => {
    expect(principalCapWarning('principal', 7)).toBe(labels.teacher.principalCapWarning)
    expect(principalCapWarning('deputy_principal', 6)).toBeNull()
    expect(principalCapWarning('none', 30)).toBeNull()
  })

  it('müdür kadrosunda saatleri 6 yapar, diğerlerinde dokunmaz', () => {
    expect(principalLoadDefaults('deputy_principal')).toEqual({ baseHours: 6, maxExtraHours: 6 })
    expect(principalLoadDefaults('department')).toBeNull()
  })

  it('Türkçe harf sırasını korur: Işık, İlker, Ömer, Özge', () => {
    const keys = turkishSortKeys(['Özge', 'İlker', 'Ömer', 'Ali', 'Işık'])
    const ordered = [...keys.entries()].sort((a, b) => a[1] - b[1]).map(([name]) => name)
    expect(ordered).toEqual(['Ali', 'Işık', 'İlker', 'Ömer', 'Özge'])
  })

  it('rütbe anahtarı müdürden öğretmene doğru artar', () => {
    const order: ChiefType[] = ['principal', 'deputy_principal', 'department', 'workshop_lab', 'none']
    const ranks = buildTeacherRows(order.map((c, i) => teacher(i, 'A', c))).map((r) => r.chiefRank)
    expect(ranks).toEqual([0, 1, 2, 3, 4])
  })
})
