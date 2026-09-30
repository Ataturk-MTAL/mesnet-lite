import { storeToRefs } from 'pinia'
import { useToast } from 'openvue/usetoast'
import { labels } from '../i18n/labels'
import { useUpdaterStore } from '../stores/updater'

function describe(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return labels.update.unknownError
}

/**
 * Güncelleme akışının Toast'lu yüzü. Durum `useUpdaterStore`'da paylaşılır;
 * kök bileşendeki diyalog ve Hakkında ekranı aynı mantığı bunun üzerinden kullanır.
 */
export function useUpdater() {
  const toast = useToast()
  const store = useUpdaterStore()
  const { supported, pending, dialogVisible, checking, installing, progressPercent } =
    storeToRefs(store)

  function showError(summary: string, error: unknown): void {
    toast.add({ severity: 'error', summary, detail: describe(error), life: 6000 })
  }

  /** Hakkında ekranındaki düğme: güncelse bildirir, yenisi varsa diyaloğu açar. */
  async function checkManually(): Promise<void> {
    try {
      if ((await store.checkManually()) === 'current') {
        toast.add({
          severity: 'info',
          summary: labels.update.upToDateSummary,
          detail: labels.update.upToDate,
          life: 4000,
        })
      }
    } catch (error: unknown) {
      showError(labels.update.checkFailed, error)
    }
  }

  async function installNow(): Promise<void> {
    try {
      await store.installNow()
    } catch (error: unknown) {
      showError(labels.update.installFailed, error)
    }
  }

  return {
    supported,
    pending,
    dialogVisible,
    checking,
    installing,
    progressPercent,
    loadSupported: store.loadSupported,
    checkOnStartup: store.checkOnStartup,
    checkManually,
    installNow,
    dismiss: store.dismiss,
  }
}
