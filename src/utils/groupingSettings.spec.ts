import { describe, expect, it } from 'vitest'
import {
  defaultGrouping,
  parseGroupingSettings,
  serializeGroupingSettings,
  uniqueValues,
  validateGrouping,
} from './groupingSettings'
import type { GroupingDraft } from './groupingSettings'

function draft(overrides: Partial<GroupingDraft> = {}): GroupingDraft {
  return { ...defaultGrouping(), ...overrides }
}

describe('parseGroupingSettings', () => {
  it('ayar yoksa varsayılanı verir', () => {
    expect(parseGroupingSettings({})).toEqual(defaultGrouping())
  })

  it('kayıtlı yöntemi, çapı ve grupları okur', () => {
    // Arrange
    const groups = [{ name: 'Örnek', neighborhoods: ['Kurgu'], districts: ['Deneme'] }]

    // Act
    const result = parseGroupingSettings({
      grouping_mode: 'manual',
      grouping_max_diameter_km: '3.5',
      grouping_manual_groups: JSON.stringify(groups),
    })

    // Assert
    expect(result).toEqual({ mode: 'manual', maxDiameterKm: 3.5, groups })
  })

  it('bozuk JSON ve bilinmeyen yöntemde varsayılana döner', () => {
    const result = parseGroupingSettings({ grouping_mode: '??', grouping_manual_groups: '{bozuk' })
    expect(result.mode).toBe('distance')
    expect(result.groups).toEqual([])
  })
})

describe('serializeGroupingSettings', () => {
  it('çapı ondalık ayracı NOKTA ile yazar', () => {
    const entries = serializeGroupingSettings(draft({ maxDiameterKm: 3.5 }))
    expect(entries.grouping_max_diameter_km).toBe('3.5')
    expect(entries.grouping_max_diameter_km).not.toContain(',')
  })

  it('tam sayı çapı da metin olarak yazar', () => {
    expect(serializeGroupingSettings(draft({ maxDiameterKm: 10 })).grouping_max_diameter_km).toBe('10')
  })

  it('grupları name/neighborhoods/districts şekliyle JSON yazar, boşlukları ve yinelenenleri atar', () => {
    // Arrange
    const input = draft({
      mode: 'manual',
      groups: [{ name: '  Örnek  ', neighborhoods: ['Kurgu', ' kurgu ', ''], districts: ['Deneme'] }],
    })

    // Act
    const entries = serializeGroupingSettings(input)

    // Assert
    expect(entries.grouping_mode).toBe('manual')
    expect(JSON.parse(entries.grouping_manual_groups)).toEqual([
      { name: 'Örnek', neighborhoods: ['Kurgu'], districts: ['Deneme'] },
    ])
  })
})

describe('validateGrouping', () => {
  it('varsayılan taslak geçerlidir', () => {
    expect(validateGrouping(defaultGrouping()).isValid).toBe(true)
  })

  it.each([0.4, 50.5, null])('%s çapını reddeder', (value) => {
    expect(validateGrouping(draft({ maxDiameterKm: value })).isDiameterValid).toBe(false)
  })

  it.each([0.5, 50])('%s sınır çapını kabul eder', (value) => {
    expect(validateGrouping(draft({ maxDiameterKm: value })).isDiameterValid).toBe(true)
  })

  it('boş adı, 80 karakterden uzun adı ve Türkçe harf duyarsız yinelenen adı işaretler', () => {
    // Arrange
    const group = (name: string) => ({ name, neighborhoods: [], districts: [] })
    const input = draft({
      mode: 'manual',
      groups: [group('Işık'), group('  '), group('ışık'), group('x'.repeat(81)), group('İpek'), group('ipek'), group('Ipek')],
    })

    // Act
    const { nameErrors, isValid } = validateGrouping(input)

    // Assert
    expect(nameErrors).toEqual([null, 'empty', 'duplicate', 'tooLong', null, 'duplicate', null])
    expect(isValid).toBe(false)
  })
})

describe('uniqueValues', () => {
  it('Türkçe harf duyarsız tekilleştirir ve ilk yazımı korur', () => {
    expect(uniqueValues(['Işık', 'ışık', 'IŞIK', 'Kurgu'])).toEqual(['Işık', 'Kurgu'])
  })
})
