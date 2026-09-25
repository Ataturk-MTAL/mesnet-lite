import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAuthStore } from './auth'
import { useSelectionStore } from './selection'
import type { User } from '../types/models'

const loginMock = vi.fn<(userId: number, pin: string) => Promise<boolean>>()
const listMock = vi.fn<() => Promise<User[]>>()
vi.mock('../api/users', () => ({
  usersApi: {
    login: (userId: number, pin: string) => loginMock(userId, pin),
    list: () => listMock(),
  },
}))

beforeEach(() => {
  loginMock.mockReset()
  listMock.mockReset()
})

describe('useAuthStore', () => {
  it('PIN doğruysa kullanıcıyı bellekte oturuma yazar', async () => {
    const store = useAuthStore()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    const ok = await store.signIn(1, '1234')

    expect(ok).toBe(true)
    expect(store.isAuthenticated).toBe(true)
    expect(store.currentUser).toEqual({ id: 1, name: 'Deniz ARSLAN', isActive: true })
  })

  it('PIN yanlışsa hata FIRLATMAZ, false döner ve oturum açılmaz', async () => {
    const store = useAuthStore()
    loginMock.mockResolvedValue(false)

    await expect(store.signIn(1, '0000')).resolves.toBe(false)
    expect(store.isAuthenticated).toBe(false)
    expect(listMock).not.toHaveBeenCalled()
  })

  it('signOut bellekteki oturumu temizler', async () => {
    const store = useAuthStore()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 2, name: 'Ayşe Kaya', isActive: true }])
    await store.signIn(2, '4321')
    expect(store.isAuthenticated).toBe(true)

    store.signOut()

    expect(store.isAuthenticated).toBe(false)
  })

  it('signOut ekran seçimlerini de sıfırlar', async () => {
    const store = useAuthStore()
    const selection = useSelectionStore()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 2, name: 'Ayşe Kaya', isActive: true }])
    await store.signIn(2, '4321')
    selection.selectedTeacherId = 7
    selection.studentSearch = 'ayşe'

    store.signOut()

    expect(selection.selectedTeacherId).toBeNull()
    expect(selection.studentSearch).toBe('')
  })
})

describe('refreshCurrentUser', () => {
  it('oturumdaki kullanıcı listede güncellenmişse yeni bir nesneyle değiştirir', async () => {
    const store = useAuthStore()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 1, name: 'Eski Ad', isActive: true }])
    await store.signIn(1, '1234')
    const before = store.currentUser

    store.refreshCurrentUser([{ id: 1, name: 'Yeni Ad', isActive: true }])

    expect(store.currentUser).toEqual({ id: 1, name: 'Yeni Ad', isActive: true })
    expect(store.currentUser).not.toBe(before)
  })

  it('verilen listede aynı id yoksa oturuma dokunmaz', async () => {
    const store = useAuthStore()
    loginMock.mockResolvedValue(true)
    listMock.mockResolvedValue([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])
    await store.signIn(1, '1234')

    store.refreshCurrentUser([{ id: 2, name: 'Başka Kullanıcı', isActive: true }])

    expect(store.currentUser).toEqual({ id: 1, name: 'Deniz ARSLAN', isActive: true })
  })

  it('oturum yoksa dokunmaz', () => {
    const store = useAuthStore()

    store.refreshCurrentUser([{ id: 1, name: 'Deniz ARSLAN', isActive: true }])

    expect(store.currentUser).toBeNull()
  })
})
