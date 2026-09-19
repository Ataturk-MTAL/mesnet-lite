import { call } from './client'
import type {
  ChangeOutcome,
  ChangeRequest,
  HistoryEventEntry,
  HistoryFilter,
  HistoryResponse,
  Stream,
} from '../types/models'

/**
 * Bir değişikliği hiçbir şey yazmadan önizler; sonuç `rejected`, `stale` ya
 * da `impact` + `highWater` taşıyan `preview` olur (spec §8).
 */
export function previewChange(request: ChangeRequest): Promise<ChangeOutcome> {
  return call('preview_change', { request })
}

/**
 * Önizlemede alınan `highWater` ile aynı isteği kalıcı yazar. Aradan başka
 * bir değişiklik geçtiyse sonuç `stale` döner; çağıran yeniden önizlemelidir.
 */
export function commitChange(request: ChangeRequest, expectedHighWater: number | null): Promise<ChangeOutcome> {
  return call('commit_change', { request, expectedHighWater })
}

/** Değişiklik kümelerini filtreye göre listeler; sayfalama `beforeChangeSetId` ile ilerler. */
export function listHistory(filter: HistoryFilter): Promise<HistoryResponse> {
  return call('list_history', { filter })
}

/** Tek bir öznenin (öğrenci, işletme, öğretmen) denetim kaydı; önce → sonra sırasıyla. */
export function getSubjectHistory(stream: Stream, subjectId: number, term: string): Promise<HistoryEventEntry[]> {
  return call('get_subject_history', { stream, subjectId, term })
}
