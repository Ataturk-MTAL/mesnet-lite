import { describe, expect, it } from 'vitest'
import { groupCompanies } from './companyGrouping'
import { labels } from '../i18n/labels'
import type { BoardCompany } from '../api/assignments'

function company(id: number, groupKey: string | null, groupLabel: string): BoardCompany {
  return {
    companyId: id,
    companyName: `Firma ${id}`,
    addressText: '',
    district: '',
    neighborhood: '',
    groupKey,
    groupLabel,
    oneWayDistanceKm: null,
    studentCount: 0,
    studentNames: [],
    branches: [],
    awardedHours: 0,
    isHonorary: false,
    hoursMissing: false,
    workplaceDays: [],
    assignedTeacherId: null,
    visitDay: null,
    visitHour: null,
    visitEndHour: null,
    isForced: false,
    forceReason: null,
    assignmentSource: null,
  }
}

describe('groupCompanies', () => {
  it('işletmeleri groupKey ile gruplar ve başlıkta etiket ile adedi verir', () => {
    // Arrange
    const all = [company(1, 'cluster:1', 'Örnek'), company(2, 'cluster:1', 'Örnek'), company(3, 'district:Deneme', 'Deneme')]

    // Act
    const groups = groupCompanies(all, all)

    // Assert
    expect(groups.map((g) => g.label)).toEqual(['Deneme (1)', 'Örnek (2)'])
    expect(groups[1].companies.map((c) => c.companyId)).toEqual([1, 2])
  })

  it('etiketleri Türkçe alfabetik sıralar', () => {
    // Arrange
    const all = [
      company(1, 'a', 'Zeytin'),
      company(2, 'b', 'İpek'),
      company(3, 'c', 'Işıklı'),
      company(4, 'd', 'Çarşı'),
      company(5, 'e', 'Cadde'),
    ]

    // Act
    const labelsInOrder = groupCompanies(all, all).map((g) => g.label)

    // Assert
    expect(labelsInOrder).toEqual(['Cadde (1)', 'Çarşı (1)', 'Işıklı (1)', 'İpek (1)', 'Zeytin (1)'])
  })

  it('grupsuz işletmeleri alfabetik olarak sondaki Grupsuz başlığında toplar', () => {
    // Arrange: "Zzz" etiketi bile Grupsuz'dan önce gelmeli
    const all = [company(1, null, ''), company(2, 'x', 'Zzz'), company(3, null, ''), company(4, 'y', 'Aaa')]

    // Act
    const groups = groupCompanies(all, all)

    // Assert
    expect(groups.map((g) => g.label)).toEqual(['Aaa (1)', 'Zzz (1)', `${labels.allocation.ungrouped} (2)`])
    expect(groups[2].isUngrouped).toBe(true)
    expect(groups[0].isUngrouped).toBe(false)
  })

  it('arama yalnız kartları daraltır, sayaç tüm gruptaki işletmeleri sayar', () => {
    // Arrange
    const all = [company(1, 'k', 'Kurgu'), company(2, 'k', 'Kurgu'), company(3, 'd', 'Deneme')]
    const visible = [all[0]]

    // Act
    const groups = groupCompanies(all, visible)

    // Assert
    expect(groups.map((g) => g.label)).toEqual(['Kurgu (2)'])
    expect(groups[0].companies).toHaveLength(1)
  })

  it('girdi dizilerini değiştirmez', () => {
    // Arrange
    const all = [company(2, 'b', 'B'), company(1, 'a', 'A')]
    const snapshot = [...all]

    // Act
    groupCompanies(all, all)

    // Assert
    expect(all).toEqual(snapshot)
  })

  it('işletme yoksa boş liste döner', () => {
    expect(groupCompanies([], [])).toEqual([])
  })
})
