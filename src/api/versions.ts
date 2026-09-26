import { call } from './client'
import type { Version } from '../types/models'

export const versionsApi = {
  /** En yeniden eskiye sıralı, kayıtlı tüm sürümler. */
  list: (): Promise<Version[]> => call('list_versions'),
  /**
   * Elle sürüm kaydı; ad boşsa ya da 80 karakteri aşıyorsa Rust `Validation`
   * hatası döner. `asOf` verilirse sürüm o tarihteki durumun kopyası olur;
   * verilmezse güncel durumdan kaydedilir.
   */
  create: (name: string, asOf: string | null = null): Promise<Version> => call('create_version', { name, asOf }),
  remove: (id: number): Promise<void> => call('delete_version', { id }),
}
