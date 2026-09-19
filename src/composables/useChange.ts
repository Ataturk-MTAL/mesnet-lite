import { computed, ref } from 'vue'
import { commitChange, previewChange } from '../api/history'
import type { ChangeOutcome, ChangeRequest, ImpactSummary, RejectionCode } from '../types/models'

export type ChangeStatus = 'idle' | 'previewing' | 'previewed' | 'rejected' | 'committing' | 'committed' | 'error'

/** `rejected` sonucunun durumda tutulan hâli; `ChangeOutcome`'un `status` alanı ayrıştırılmış olarak. */
export interface ChangeRejection {
  code: RejectionCode
  reason: string
  conflictingChangeSetIds: number[]
  suggestedDate: string | null
}

function toErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Tek bir değişiklik isteğinin önizle → onayla döngüsünü yönetir. `ImpactDialog`
 * bu durumu okuyup gösterir; bu dosya hiçbir UI içermez (V2'nin işi).
 *
 * Her çağıran kendi örneğini oluşturur — `useAsOfDate`'in aksine paylaşılan
 * bir modül durumu YOKTUR; aynı anda birden çok değişiklik penceresi açık
 * olabilir.
 */
export function useChange() {
  const status = ref<ChangeStatus>('idle')
  const currentRequest = ref<ChangeRequest | null>(null)
  const impact = ref<ImpactSummary | null>(null)
  const highWater = ref<number | null>(null)
  const rejection = ref<ChangeRejection | null>(null)
  const changeSetId = ref<number | null>(null)
  const errorMessage = ref<string | null>(null)

  /** Reddin `previousMonthClosed` olduğu ve bir öneri tarihi taşıdığı hâl. */
  const canUseSuggestedDate = computed(
    () =>
      status.value === 'rejected' &&
      rejection.value?.code === 'previousMonthClosed' &&
      rejection.value?.suggestedDate !== null,
  )

  /** `preview`/`commit` sonucunu duruma işler; `stale` ise aynı istekle sessizce yeniden önizler. */
  async function applyOutcome(outcome: ChangeOutcome, request: ChangeRequest): Promise<void> {
    if (outcome.status === 'preview') {
      impact.value = outcome.impact
      highWater.value = outcome.highWater
      rejection.value = null
      status.value = 'previewed'
      return
    }
    if (outcome.status === 'rejected') {
      rejection.value = {
        code: outcome.code,
        reason: outcome.reason,
        conflictingChangeSetIds: outcome.conflictingChangeSetIds,
        suggestedDate: outcome.suggestedDate,
      }
      impact.value = null
      highWater.value = null
      status.value = 'rejected'
      return
    }
    if (outcome.status === 'committed') {
      impact.value = outcome.impact
      changeSetId.value = outcome.changeSetId
      status.value = 'committed'
      return
    }
    // outcome.status === 'stale': aradan başka bir değişiklik geçmiş; aynı istekle otomatik yeniden önizlenir.
    await preview(request)
  }

  /** İsteği hiçbir şey yazmadan önizler. */
  async function preview(request: ChangeRequest): Promise<void> {
    currentRequest.value = request
    status.value = 'previewing'
    errorMessage.value = null
    try {
      const outcome = await previewChange(request)
      await applyOutcome(outcome, request)
    } catch (error: unknown) {
      status.value = 'error'
      errorMessage.value = toErrorMessage(error)
    }
  }

  /** Son önizlenen isteği, önizlemede alınan `highWater` ile kalıcı yazar. */
  async function confirm(): Promise<void> {
    const request = currentRequest.value
    if (!request || status.value !== 'previewed') return
    status.value = 'committing'
    errorMessage.value = null
    try {
      const outcome = await commitChange(request, highWater.value)
      await applyOutcome(outcome, request)
    } catch (error: unknown) {
      status.value = 'error'
      errorMessage.value = toErrorMessage(error)
    }
  }

  /**
   * `previousMonthClosed` reddinden sonra "bu ayın 1'inden gir" seçeneği.
   * Kullanıcının girdiği özgün tarih `documentDate` alanında saklanır.
   */
  async function useSuggestedDate(): Promise<void> {
    const request = currentRequest.value
    const suggested = rejection.value?.suggestedDate ?? null
    if (!request || !suggested || rejection.value?.code !== 'previousMonthClosed') return
    await preview({ ...request, effectiveDate: suggested, documentDate: request.effectiveDate })
  }

  /** Pencere kapanırken durumu temizler. */
  function reset(): void {
    status.value = 'idle'
    currentRequest.value = null
    impact.value = null
    highWater.value = null
    rejection.value = null
    changeSetId.value = null
    errorMessage.value = null
  }

  return {
    status,
    impact,
    highWater,
    rejection,
    changeSetId,
    errorMessage,
    canUseSuggestedDate,
    preview,
    confirm,
    useSuggestedDate,
    reset,
  }
}
