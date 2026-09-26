import { describe, expect, it } from 'vitest'
import { useChangeDetailsDialog } from './useChangeDetailsDialog'

describe('useChangeDetailsDialog', () => {
  it('planlamada pencere açmadan boş tarih/gerekçe döner', async () => {
    // Arrange
    const dialog = useChangeDetailsDialog(() => true)

    // Act
    const details = await dialog.requestDetails()

    // Assert
    expect(dialog.isOpen.value).toBe(false)
    expect(details).toEqual({ effectiveDate: null, reason: null })
  })

  it('dönem başladıysa pencereyi açar ve onayda tarih/gerekçeyi döner', async () => {
    // Arrange
    const dialog = useChangeDetailsDialog(() => false)

    // Act
    const pending = dialog.requestDetails()
    expect(dialog.isOpen.value).toBe(true)
    dialog.confirm({ effectiveDate: '2026-10-01', reason: 'Test gerekçesi' })
    const details = await pending

    // Assert
    expect(dialog.isOpen.value).toBe(false)
    expect(details).toEqual({ effectiveDate: '2026-10-01', reason: 'Test gerekçesi' })
  })

  it('vazgeçilirse null döner, pencere kapanır', async () => {
    // Arrange
    const dialog = useChangeDetailsDialog(() => false)

    // Act
    const pending = dialog.requestDetails()
    dialog.cancel()
    const details = await pending

    // Assert
    expect(dialog.isOpen.value).toBe(false)
    expect(details).toBeNull()
  })

  it('son onaylanan tarih bir sonraki açılışta ön değer olur', async () => {
    // Arrange
    const dialog = useChangeDetailsDialog(() => false)
    const firstPending = dialog.requestDetails()
    dialog.confirm({ effectiveDate: '2026-10-05', reason: 'İlk' })
    await firstPending

    // Act
    dialog.requestDetails()

    // Assert
    expect(dialog.lastEffectiveDate.value).toBe('2026-10-05')
  })
})
