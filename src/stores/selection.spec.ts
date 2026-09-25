import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useSelectionStore } from './selection'

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('useSelectionStore syncTeacherSelection', () => {
  it('mevcut kimlik listede varsa dokunmaz', () => {
    // Arrange
    const selection = useSelectionStore()
    selection.selectedTeacherId = 2

    // Act
    selection.syncTeacherSelection([1, 2, 3])

    // Assert
    expect(selection.selectedTeacherId).toBe(2)
  })

  it('kimlik listede yoksa (bayat) listenin ilkine düşer', () => {
    // Arrange
    const selection = useSelectionStore()
    selection.selectedTeacherId = 99

    // Act
    selection.syncTeacherSelection([1, 2, 3])

    // Assert
    expect(selection.selectedTeacherId).toBe(1)
  })

  it('liste boşsa null olur', () => {
    // Arrange
    const selection = useSelectionStore()
    selection.selectedTeacherId = 1

    // Act
    selection.syncTeacherSelection([])

    // Assert
    expect(selection.selectedTeacherId).toBeNull()
  })

  it('seçim hiç yapılmamışsa (null) ilk öğretmene düşer', () => {
    // Arrange
    const selection = useSelectionStore()

    // Act
    selection.syncTeacherSelection([5, 6])

    // Assert
    expect(selection.selectedTeacherId).toBe(5)
  })
})

describe('useSelectionStore syncHistoryOptions', () => {
  it('bayat işletme/öğretmen kimliğini temizler', () => {
    // Arrange
    const selection = useSelectionStore()
    selection.historyCompanyId = 99
    selection.historyTeacherId = 88

    // Act
    selection.syncHistoryOptions([1, 2], [3, 4])

    // Assert
    expect(selection.historyCompanyId).toBeNull()
    expect(selection.historyTeacherId).toBeNull()
  })

  it('geçerli kimlikleri korur', () => {
    // Arrange
    const selection = useSelectionStore()
    selection.historyCompanyId = 1
    selection.historyTeacherId = 3

    // Act
    selection.syncHistoryOptions([1, 2], [3, 4])

    // Assert
    expect(selection.historyCompanyId).toBe(1)
    expect(selection.historyTeacherId).toBe(3)
  })

  it('null filtreye dokunmaz', () => {
    // Arrange
    const selection = useSelectionStore()

    // Act
    selection.syncHistoryOptions([1, 2], [3, 4])

    // Assert
    expect(selection.historyCompanyId).toBeNull()
    expect(selection.historyTeacherId).toBeNull()
  })
})

describe('useSelectionStore reset', () => {
  it('her alanı varsayılana döndürür', () => {
    // Arrange
    const selection = useSelectionStore()
    selection.selectedTeacherId = 1
    selection.historyStream = 'coordination'
    selection.historySubjectId = 7
    selection.historyCompanyId = 2
    selection.historyTeacherId = 3
    selection.historyIncludeOpening = true
    selection.studentSearch = 'ayşe'
    selection.companySearch = 'firma'
    selection.companyHoursSearch = 'akdeniz'
    selection.allocationCompanySearch = 'toroslar'

    // Act
    selection.reset()

    // Assert
    expect(selection.selectedTeacherId).toBeNull()
    expect(selection.historyStream).toBeNull()
    expect(selection.historySubjectId).toBeNull()
    expect(selection.historyCompanyId).toBeNull()
    expect(selection.historyTeacherId).toBeNull()
    expect(selection.historyIncludeOpening).toBe(false)
    expect(selection.studentSearch).toBe('')
    expect(selection.companySearch).toBe('')
    expect(selection.companyHoursSearch).toBe('')
    expect(selection.allocationCompanySearch).toBe('')
  })
})
