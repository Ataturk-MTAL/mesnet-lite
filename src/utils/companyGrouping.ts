import type { BoardCompany } from '../api/assignments'
import { labels } from '../i18n/labels'

/** Atama ekranındaki tek bir grup başlığı ve altındaki kartlar. */
export interface CompanyGroup {
  /** `groupKey`; grupsuz işletmeler için boş dize (gerçek anahtarlar hiçbir zaman boş değildir). */
  key: string
  /** "Etiket (adet)" — adet arama filtresinden ETKİLENMEZ, gruptaki TÜM işletmeleri sayar. */
  label: string
  /** Aramayı geçen, gösterilecek kartlar. */
  companies: BoardCompany[]
  isUngrouped: boolean
}

const UNGROUPED_KEY = ''

function keyOf(company: BoardCompany): string {
  return company.groupKey ?? UNGROUPED_KEY
}

/** Grup başlığı için gösterilecek ad; etiketi boş gelen gruba anahtarı yedek olur. */
function displayName(key: string, firstCompany: BoardCompany): string {
  if (key === UNGROUPED_KEY) return labels.allocation.ungrouped
  return firstCompany.groupLabel.trim().length > 0 ? firstCompany.groupLabel : key
}

/**
 * Atanmamış işletmeleri `groupKey`'e göre gruplar. Gruplar etikete göre Türkçe
 * alfabetik sıralanır; `groupKey === null` olanlar en sonda "Grupsuz" başlığında
 * toplanır. `all` sayaçları, `visible` ise gösterilecek kartları verir; görünür
 * kartı kalmayan grup hiç döndürülmez. Girdileri değiştirmez.
 */
export function groupCompanies(all: BoardCompany[], visible: BoardCompany[]): CompanyGroup[] {
  const totals = new Map<string, BoardCompany[]>()
  for (const company of all) {
    totals.set(keyOf(company), [...(totals.get(keyOf(company)) ?? []), company])
  }

  const groups: CompanyGroup[] = []
  for (const [key, members] of totals) {
    const shown = visible.filter((company) => keyOf(company) === key)
    if (shown.length === 0) continue
    groups.push({
      key,
      label: `${displayName(key, members[0])} (${members.length})`,
      companies: shown,
      isUngrouped: key === UNGROUPED_KEY,
    })
  }

  return groups.sort((a, b) => {
    if (a.isUngrouped !== b.isUngrouped) return a.isUngrouped ? 1 : -1
    return a.label.localeCompare(b.label, 'tr')
  })
}
