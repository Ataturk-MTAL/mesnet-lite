// Kullanım: node shot.mjs <rota> <çıktı.png> [genişlik] [yükseklik] [fixture.json]
// Vite (http://localhost:1420) sayfasını gerçek Chrome ile çizer; Tauri IPC'yi
// fixture'la taklit eder. Bilinmeyen komutlar konsola yazılır ve `null` döner.
import { chromium } from 'playwright-core'
import { readFileSync } from 'node:fs'

const [route = '/', out = 'shot.png', width = '1400', height = '900', fixturePath] = process.argv.slice(2)
const fixtures = fixturePath ? JSON.parse(readFileSync(fixturePath, 'utf8')) : {}

const browser = await chromium.launch({ channel: 'chrome', headless: true })
const page = await browser.newPage({ viewport: { width: Number(width), height: Number(height) } })

const unknown = new Set()
page.on('console', (message) => {
  if (message.type() === 'error') console.log('[console.error]', message.text().slice(0, 200))
})
page.on('pageerror', (error) => console.log('[pageerror]', String(error).slice(0, 200)))

await page.exposeFunction('__mock', (command, args) => {
  if (command in fixtures) return fixtures[command]
  unknown.add(command)
  return null
})

await page.addInitScript(() => {
  window.__TAURI_INTERNALS__ = {
    invoke: (command, args) => window.__mock(command, args),
    transformCallback: () => 0,
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
  }
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
})

// THEME=dark koyu temayı (`app-dark`) açar; CLICK="metin" görünür bir düğmeye
// tıklar (örn. bir diyaloğu açmak için) ve ekran görüntüsünü ondan sonra alır.
if (process.env.THEME === 'dark') {
  // documentElement init script anında henüz yok; uygulama temayı localStorage'tan okur (useTheme.ts).
  await page.addInitScript(() => localStorage.setItem('mesnet-lite-theme', 'dark'))
}

await page.goto(`http://localhost:1420${route}`, { waitUntil: 'networkidle' })
await page.waitForTimeout(1500)
if (process.env.CLICK) {
  // Birden çok tıklama için metinleri `|` ile ayırın: CLICK="Tümünü Dolu Yap|Boş Saatleri Kaydet"
  for (const text of process.env.CLICK.split('|')) {
    // "Kaydet@last" gibi bir ek, aynı metne uyan SON öğeyi (örn. diyalogdaki düğmeyi) seçer.
    const isLast = text.endsWith('@last')
    const locator = page.getByText(isLast ? text.slice(0, -5) : text, { exact: false })
    await (isLast ? locator.last() : locator.first()).click()
    await page.waitForTimeout(600)
  }
}
if (process.env.HOVER) {
  // Fareyle açılan ipucunu (tooltip) ekran görüntüsünde görünür kılmak için üzerine gelir.
  const locator = page.getByText(process.env.HOVER, { exact: false })
  await locator.first().hover()
  await page.waitForTimeout(700)
}
await page.screenshot({ path: out, fullPage: true })
console.log('saved', out)
if (unknown.size) console.log('unmocked commands:', [...unknown].join(', '))
await browser.close()
