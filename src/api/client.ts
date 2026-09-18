import { invoke } from '@tauri-apps/api/core'

/**
 * Bilinmeyen bir hata değerinden okunabilir mesaj çıkarır.
 * Rust tarafı AppError'ı metin olarak serileştirir; ancak Tauri çalışma zamanı
 * yoksa (ör. uygulama düz tarayıcıda açıldığında) bir Error nesnesi gelir.
 * Hiçbir durumda asıl mesaj kaybedilmez.
 */
function toMessage(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  if (error && typeof error === 'object') {
    try {
      return JSON.stringify(error)
    } catch {
      return String(error)
    }
  }
  return String(error)
}

/**
 * Tauri komutlarını çağırır ve Rust'tan gelen Türkçe hata metnini olduğu gibi
 * yükseltir. Hata sessizce yutulmaz; çağıran katman Toast ile gösterir.
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error: unknown) {
    // Hangi komutun düştüğü teşhis için mesaja eklenir.
    throw new Error(`${command}: ${toMessage(error)}`)
  }
}
