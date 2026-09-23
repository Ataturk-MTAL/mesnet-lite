import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { User } from '../types/models'

const loginMock = vi.fn<(userId: number, pin: string) => Promise<boolean>>()
const listMock = vi.fn<() => Promise<User[]>>()
vi.mock('../api/users', () => ({
  usersApi: {
    login: (userId: number, pin: string) => loginMock(userId, pin),
    list: () => listMock(),
  },
}))

// Modül düzeyinde paylaşılan durum taşıdığı için her testte taze modül alınır.
async function loadAuth() {
  vi.resetModules()
  return import('./useAuth')
}

beforeEach(() => {
  loginMock.mockReset()
  listMock.mockReset()
})

describe('useAuth', () => {
  it('PIN doğruysa kullanıcıyı bellekte oturuma yazar', async () => {
    const { signIn, currentUser, isAuthenticated } = await loadAuth()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    const ok = await signIn(1, '1234')

    expect(ok).toBe(true)
    expect(isAuthenticated.value).toBe(true)
    expect(currentUser.value).toEqual({ id: 1, name: 'Deniz ARSLAN', isActive: true })
  })

  it('PIN yanlışsa hata FIRLATMAZ, false döner ve oturum açılmaz', async () => {
    const { signIn, isAuthenticated } = await loadAuth()
    loginMock.mockResolvedValue(false)

    await expect(signIn(1, '0000')).resolves.toBe(false)
    expect(isAuthenticated.value).toBe(false)
    expect(listMock).not.toHaveBeenCalled()
  })

  it('signOut bellekteki oturumu temizler', async () => {
    const { signIn, signOut, isAuthenticated } = await loadAuth()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 2, name: 'Ayşe Kaya', isActive: true }])
    await signIn(2, '4321')
    expect(isAuthenticated.value).toBe(true)

    signOut()

    expect(isAuthenticated.value).toBe(false)
  })
})

describe('refreshCurrentUser', () => {
  it('oturumdaki kullanıcı listede güncellenmişse yeni bir nesneyle değiştirir', async () => {
    const { signIn, currentUser, refreshCurrentUser } = await loadAuth()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 1, name: 'Eski Ad', isActive: true }])
    await signIn(1, '1234')
    const before = currentUser.value

    refreshCurrentUser([{ id: 1, name: 'Yeni Ad', isActive: true }])

    expect(currentUser.value).toEqual({ id: 1, name: 'Yeni Ad', isActive: true })
    expect(currentUser.value).not.toBe(before)
  })

  it('verilen listede aynı id yoksa oturuma dokunmaz', async () => {
    const { signIn, currentUser, refreshCurrentUser } = await loadAuth()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])
    await signIn(1, '1234')

    refreshCurrentUser([{ id: 2, name: 'Başka Kullanıcı', isActive: true }])

    expect(currentUser.value).toEqual({ id: 1, name: 'Deniz ARSLAN', isActive: true })
  })

  it('oturum yoksa dokunmaz', async () => {
    const { currentUser, refreshCurrentUser } = await loadAuth()

    refreshCurrentUser([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    expect(currentUser.value).toBeNull()
  })
})
