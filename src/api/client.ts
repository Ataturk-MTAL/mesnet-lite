import { invoke } from '@tauri-apps/api/core'

/**
 * Tauri komutlarını çağırır ve Rust'tan gelen Türkçe hata metnini olduğu gibi
 * yükseltir. Hata sessizce yutulmaz; çağıran katman Toast ile gösterir.
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error: unknown) {
    const message = typeof error === 'string' ? error : 'Beklenmeyen hata'
    throw new Error(message)
  }
}
