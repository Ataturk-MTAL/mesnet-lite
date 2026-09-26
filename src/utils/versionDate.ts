// `Version.createdAt` zaten Türkiye saatiyle 'YYYY-MM-DDTHH:MM:SS' biçiminde
// gelir. `Date` nesnesine çevirmeden doğrudan dizgi üzerinde çalışılır; aksi
// hâlde tarayıcının saat dilimi yorumu günü/saati kaydırabilir.

/** Ekranda göstermek için 'dd.MM.yyyy HH:mm' biçimine çevirir. */
export function formatVersionTimestamp(createdAt: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(createdAt)
  if (!match) return createdAt
  const [, year, month, day, hour, minute] = match
  return `${day}.${month}.${year} ${hour}:${minute}`
}

/** Dosya adı önerisine eklenecek, sürümün tarihini taşıyan 'YYYY-MM-DD' parçası. */
export function versionDateForFileName(createdAt: string): string {
  const match = /^(\d{4}-\d{2}-\d{2})T/.exec(createdAt)
  return match ? match[1] : createdAt
}

/** `Version.asOf` gibi salt 'YYYY-MM-DD' tarihlerini 'dd.MM.yyyy' biçimine çevirir. */
export function formatAsOfDate(asOf: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(asOf)
  if (!match) return asOf
  const [, year, month, day] = match
  return `${day}.${month}.${year}`
}
