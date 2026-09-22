import { call } from './client'
import type { BackupStatus } from '../types/models'

/** Ayarlar ekranındaki "Yedekleme" kartının kullandığı Tauri komutları. */
export const backupApi = {
  status: (): Promise<BackupStatus> => call('backup_status'),
  create: (path: string): Promise<void> => call('create_backup', { path }),
  /** Başarıda uygulama kendiliğinden yeniden başlar; dönüş hiç gelmeyebilir. */
  restore: (path: string): Promise<void> => call('restore_backup', { path }),
}
