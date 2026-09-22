import { describe, expect, it } from 'vitest'
import { sanitizePinInput } from './pin'

describe('sanitizePinInput', () => {
  it('rakam olmayan karakterleri atar', () => {
    expect(sanitizePinInput('1a2b3c')).toBe('123')
  })

  it('altıncı haneden sonrasını kırpar', () => {
    expect(sanitizePinInput('1234567890')).toBe('123456')
  })

  it('boş ya da tanımsız girdide boş dize döner', () => {
    expect(sanitizePinInput('')).toBe('')
    expect(sanitizePinInput(null)).toBe('')
    expect(sanitizePinInput(undefined)).toBe('')
  })
})
