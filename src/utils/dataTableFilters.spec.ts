import { describe, expect, it } from 'vitest'
import { buildGlobalFilter, extractGlobalFilterValue } from './dataTableFilters'

describe('buildGlobalFilter', () => {
  it('boş olmayan aramayı value olarak taşır', () => {
    // Arrange & Act
    const filter = buildGlobalFilter('ayşe')

    // Assert
    expect(filter).toEqual({ global: { value: 'ayşe', matchMode: 'contains' } })
  })

  it('boş aramada value null olur', () => {
    // Arrange & Act
    const filter = buildGlobalFilter('')

    // Assert
    expect(filter).toEqual({ global: { value: null, matchMode: 'contains' } })
  })
})

describe('extractGlobalFilterValue', () => {
  it('string değeri olduğu gibi döndürür', () => {
    // Arrange & Act
    const value = extractGlobalFilterValue({ global: { value: 'firma', matchMode: 'contains' } })

    // Assert
    expect(value).toBe('firma')
  })

  it('null değerde boş dize döndürür', () => {
    // Arrange & Act
    const value = extractGlobalFilterValue({ global: { value: null, matchMode: 'contains' } })

    // Assert
    expect(value).toBe('')
  })

  it('global alanı yoksa boş dize döndürür', () => {
    // Arrange & Act
    const value = extractGlobalFilterValue({})

    // Assert
    expect(value).toBe('')
  })
})
