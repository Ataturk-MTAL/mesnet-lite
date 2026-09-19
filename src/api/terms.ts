import { call } from './client'

/** `create_term` çağrısının sonucu. */
export interface CreateTermResult {
  term: string
  /** Dönem çağrıdan önce de bilinen dönemler arasındaysa `true`; bu bir hata değildir. */
  alreadyExisted: boolean
  /** Devir istendiyse ve hedefte satır yoksa kopyalanan ders yükü satırı sayısı; aksi hâlde 0. */
  teachingLoadRowsCopied: number
  /** Devir istendiği hâlde hedefte zaten satır olduğu için atlandıysa `true`. */
  teachingLoadCopySkipped: boolean
}

export const termsApi = {
  /**
   * Bilinen tüm dönemler (döneme bağlı her tablonun birleşimi + aktif dönem),
   * en yeniden eskiye. Henüz öğrencisi olmayan yeni bir eğitim-öğretim yılı
   * da — başka bir tabloda verisi varsa veya aktifse — bu listede görünür.
   */
  list: (): Promise<string[]> => call('get_known_terms'),
  /**
   * Yeni bir dönem kaydeder; biçim `YYYY-YYYY/N` olmalı. `copyTeachingLoadFromTerm`
   * verilirse kaynak dönemin ders yükü satırları hedefe kopyalanır — ama
   * yalnızca hedefte zaten satır YOKSA.
   */
  create: (term: string, copyTeachingLoadFromTerm: string | null): Promise<CreateTermResult> =>
    call('create_term', { term, copyTeachingLoadFromTerm }),
}
