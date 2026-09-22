import { call } from './client'
import type { User } from '../types/models'

export const usersApi = {
  /** Veritabanında hiç kullanıcı yoksa `false`; ilk kurulum ekranı bu duruma göre çizilir. */
  hasAny: (): Promise<boolean> => call('has_any_user'),
  /** Pasif kullanıcılar da dahil TÜM kullanıcılar; filtreleme çağıran katmanda yapılır. */
  list: (): Promise<User[]> => call('list_users'),
  create: (name: string, pin: string): Promise<User> => call('create_user', { name, pin }),
  rename: (id: number, name: string): Promise<void> => call('rename_user', { id, name }),
  setPin: (id: number, pin: string): Promise<void> => call('set_user_pin', { id, pin }),
  setActive: (id: number, isActive: boolean): Promise<void> => call('set_user_active', { id, isActive }),
  /**
   * PIN doğruysa `true` döner ve arka uç `settings.operator_name`i günceller;
   * bundan sonraki her değişiklik tarihçeye bu adla düşer. Yanlış PIN'de hata
   * FIRLATMAZ, `false` döner — çağıran taraf bunu "PIN hatalı" diye gösterir.
   */
  login: (userId: number, pin: string): Promise<boolean> => call('login', { userId, pin }),
}
