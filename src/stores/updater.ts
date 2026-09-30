import { defineStore } from 'pinia'
import { ref, shallowRef } from 'vue'
import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { updaterApi } from '../api/updater'

/** Elle denetimin sonucu: yeni sürüm bulundu mu, yoksa uygulama güncel mi? */
export type ManualCheckResult = 'available' | 'current'

const MAX_PERCENT = 100

/**
 * Otomatik güncelleme durumu. Diyalog kök bileşende, "Güncellemeleri Denetle"
 * düğmesi Hakkında ekranında durur; ikisi aynı durumu paylaşır, bu yüzden
 * burada yaşar. Hata bildirimi (Toast) `useUpdater` composable'ındadır.
 */
export const useUpdaterStore = defineStore('updater', () => {
  /** `null`: henüz sorulmadı. */
  const supported = ref<boolean | null>(null)
  // `Update` bir Tauri Resource'udur; derin reaktif proxy'ye sarılmamalı.
  const pending = shallowRef<Update | null>(null)
  const dialogVisible = ref(false)
  const checking = ref(false)
  const installing = ref(false)
  /** `null`: toplam boyut bilinmiyor (belirsiz ilerleme) ya da indirme başlamadı. */
  const progressPercent = ref<number | null>(null)
  const hasStartupChecked = ref(false)

  async function loadSupported(): Promise<boolean> {
    if (supported.value !== null) return supported.value
    try {
      supported.value = await updaterApi.isSupported()
    } catch (error: unknown) {
      // Komut yoksa ya da düştüyse güncelleme yapılamaz kabul edilir.
      console.error('updater_supported başarısız:', error)
      supported.value = false
    }
    return supported.value
  }

  function offer(update: Update): void {
    pending.value = update
    dialogVisible.value = true
  }

  /** Açılışta bir kez, sessizce denetler; hatalar yalnız konsola yazılır. */
  async function checkOnStartup(): Promise<void> {
    if (hasStartupChecked.value) return
    hasStartupChecked.value = true
    if (!(await loadSupported())) return
    try {
      const update = await check()
      if (update) offer(update)
    } catch (error: unknown) {
      console.error('Güncelleme denetimi başarısız:', error)
    }
  }

  /** Kullanıcı isteğiyle denetler; hata çağırana yükseltilir. */
  async function checkManually(): Promise<ManualCheckResult> {
    checking.value = true
    try {
      const update = await check()
      if (!update) return 'current'
      offer(update)
      return 'available'
    } finally {
      checking.value = false
    }
  }

  function makeProgressHandler(): (event: DownloadEvent) => void {
    let total: number | null = null
    let downloaded = 0
    return (event) => {
      if (event.event === 'Started') {
        total = event.data.contentLength ?? null
        downloaded = 0
        progressPercent.value = total ? 0 : null
      } else if (event.event === 'Progress' && total) {
        downloaded += event.data.chunkLength
        const percent = Math.min(MAX_PERCENT, Math.floor((downloaded * MAX_PERCENT) / total))
        // Sık tetiklenen olay: yüzde değişmediyse yazma.
        if (percent !== progressPercent.value) progressPercent.value = percent
      } else if (event.event === 'Finished' && total) {
        progressPercent.value = MAX_PERCENT
      }
    }
  }

  /** İndirir, kurar ve yeniden başlatır; hata çağırana yükseltilir. */
  async function installNow(): Promise<void> {
    const update = pending.value
    if (!update || installing.value) return
    installing.value = true
    progressPercent.value = null
    try {
      await update.downloadAndInstall(makeProgressHandler())
      await relaunch()
    } finally {
      installing.value = false
    }
  }

  /** "Sonra": diyalog kapanır; açılış denetimi oturumda bir kez yapıldığı için yeniden sorulmaz. */
  function dismiss(): void {
    dialogVisible.value = false
  }

  return {
    supported,
    pending,
    dialogVisible,
    checking,
    installing,
    progressPercent,
    loadSupported,
    checkOnStartup,
    checkManually,
    installNow,
    dismiss,
  }
})
