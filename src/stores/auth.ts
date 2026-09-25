import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { usersApi } from '../api/users'
import { useSelectionStore } from './selection'
import type { User } from '../types/models'

/**
 * Oturum durumu YALNIZ BELLEKTE tutulur — `localStorage`/`sessionStorage`
 * KULLANILMAZ. Uygulama kapanıp açıldığında yeniden giriş istenir; kalıcı
 * oturum "değişikliği kim yaptı" tarihçesinin amacını boşa çıkarır ve bu
 * masaüstü uygulamada kalıcılığın koruma değeri yoktur.
 */
export const useAuthStore = defineStore('auth', () => {
  const currentUser = ref<User | null>(null)
  const isAuthenticated = computed(() => currentUser.value !== null)

  /**
   * Seçilen kullanıcı için PIN doğrular. `login` komutu PIN doğruysa arka uçta
   * `settings.operator_name`i günceller; bundan sonraki her değişiklik
   * tarihçeye bu adla düşer. PIN yanlışsa `false` döner, hata FIRLATILMAZ.
   * Doğruysa kullanıcı listesinden aynı kaydı bulup bellekteki oturuma yazar.
   */
  async function signIn(userId: number, pin: string): Promise<boolean> {
    const isPinCorrect = await usersApi.login(userId, pin)
    if (!isPinCorrect) return false

    const users = await usersApi.list()
    const matched = users.find((user) => user.id === userId) ?? null
    currentUser.value = matched
    return matched !== null
  }

  /**
   * Bellekteki oturumu temizler; uygulama kabuğu kapanır ve giriş ekranı
   * döner. Ekran seçimleri (öğretmen seçimi, tarihçe filtreleri, aramalar)
   * de burada sıfırlanır; başka bir kullanıcı öncekinin filtrelerini devralmaz.
   */
  function signOut(): void {
    currentUser.value = null
    useSelectionStore().reset()
  }

  /**
   * Oturumdaki kullanıcı listede güncellenmiş bir kayıtla eşleşiyorsa (ör. adı
   * değişti) bellekteki oturumu YENİ bir nesneyle değiştirir. Oturum yoksa veya
   * verilen listede aynı `id` bulunamıyorsa dokunmaz — `rename_user` sonrası
   * `SettingsView.loadUsers()` her çağrıda bunu tetikler.
   */
  function refreshCurrentUser(users: readonly User[]): void {
    const current = currentUser.value
    if (current === null) return
    const matched = users.find((user) => user.id === current.id)
    if (matched === undefined) return
    currentUser.value = { ...matched }
  }

  return { currentUser, isAuthenticated, signIn, signOut, refreshCurrentUser }
})
