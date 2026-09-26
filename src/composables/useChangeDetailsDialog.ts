import { ref } from 'vue'
import type { EffectiveChangeInput } from '../types/models'

/**
 * "Kaydet", "Ata" ya da "Çıkar" gibi doğrudan bir yazım eylemini, dönem
 * başladıysa (`isPlanning() === false`) tarih ve gerekçe soran
 * `ChangeDetailsDialog`in arkasına koyar.
 *
 * Planlama evresinde pencere hiç açılmaz; `requestDetails` doğrudan boş
 * (`effectiveDate: null, reason: null`) bir sonuç döner. Kullanıcı pencereyi
 * Vazgeç ile kapatırsa `requestDetails` `null` döner; çağıran eylemi tamamen
 * iptal etmelidir (arka uca hiçbir şey gitmez).
 *
 * `lastEffectiveDate`, aynı görünüm oturumu içinde ard arda yapılan
 * yazımlarda pencerenin ön değeri olur — kullanıcı aynı tarihi tekrar
 * seçmek zorunda kalmaz. Gerekçe her açılışta boş başlar.
 *
 * Her çağıran kendi örneğini oluşturur; `useChange`'in aksine bileşene özel,
 * paylaşılan bir durum YOKTUR.
 */
export function useChangeDetailsDialog(isPlanning: () => boolean) {
  const isOpen = ref(false)
  const lastEffectiveDate = ref<string | null>(null)

  let pendingResolve: ((details: EffectiveChangeInput | null) => void) | null = null

  /** Planlamadaysa pencere açılmadan boş sonuç döner; aksi hâlde pencere açılır
   *  ve kullanıcı onaylayana/vazgeçene kadar bekletir. */
  function requestDetails(): Promise<EffectiveChangeInput | null> {
    if (isPlanning()) {
      return Promise.resolve({ effectiveDate: null, reason: null })
    }
    isOpen.value = true
    return new Promise<EffectiveChangeInput | null>((resolve) => {
      pendingResolve = resolve
    })
  }

  /** `ChangeDetailsDialog`in `confirm` olayına bağlanır. */
  function confirm(details: { effectiveDate: string | null; reason: string }): void {
    isOpen.value = false
    lastEffectiveDate.value = details.effectiveDate
    pendingResolve?.({ effectiveDate: details.effectiveDate, reason: details.reason })
    pendingResolve = null
  }

  /** `ChangeDetailsDialog`in `cancel` olayına bağlanır. */
  function cancel(): void {
    isOpen.value = false
    pendingResolve?.(null)
    pendingResolve = null
  }

  return { isOpen, lastEffectiveDate, requestDetails, confirm, cancel }
}
