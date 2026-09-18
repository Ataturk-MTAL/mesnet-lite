# MESNET.Lite Faz 1 — Temel ve Veri Katmanı Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tauri 2 masaüstü uygulamasının çalışan iskeletini, kalıcı SQLite veri katmanını, OpenVue tabanlı kenar çubuğu navigasyonunu ve İşletme / Öğrenci / Öğretmen kayıtlarının tam CRUD'unu, CSV içe aktarma ve harita üzerinde konum düzeltme ile birlikte kurmak.

**Architecture:** Rust backend `db/` (depo katmanı), `domain/` (saf tipler ve hesap), `services/` (CSV, coğrafi kodlama) ve `commands/` (ince Tauri sarmalayıcıları) olarak katmanlanır; `commands/` iş mantığı içermez. Frontend Vue 3 + TypeScript, kenar çubuğu kabuğu içinde router görünümleri barındırır ve backend'e yalnızca `src/api/` altındaki tipli `invoke` sarmalayıcıları üzerinden erişir.

**Tech Stack:** Tauri 2 · Rust · `sqlx` 0.8 (SQLite) · `thiserror` · `serde` · `reqwest` · Vue 3.5 · TypeScript · Vite · OpenVue 1.0.0 + `@openvue/themes` 1.0.0 · Pinia · Leaflet 1.9 · Vitest

## Global Constraints

Bu bölüm her görevin gereksinimlerine örtük olarak dahildir.

- **Dil kuralı:** Dosya adları, tablo/sütun adları, tip/fonksiyon/değişken adları ve enum değerleri **İngilizce**. Kod içi yorumlar, dokümantasyon, arayüz metinleri ve hata mesajları **Türkçe**. Arayüz etiketleri yalnızca `src/i18n/labels.ts` içinde tanımlanır.
- **Paket sürümleri kesindir:** `openvue@1.0.0`, `@openvue/themes@1.0.0`, `primeicons`. **PrimeVue KULLANILMAZ:** v5 çalışma zamanında lisans anahtarı doğrular ve anahtarsız çalıştırıldığında tüm stiller kaybolup `Invalid PrimeUI License` rozeti çıkar. OpenVue, PrimeVue 4.5.5'in MIT forkudur ve anahtar istemez.
- **Kök font boyutu 14px'tir** (OpenVue, PrimeVue v4 tabanlı).
- **Hazır takvim/zamanlama bileşeni yoktur.** Faz 2'deki müsaitlik ızgarası elle yazılır.
- **Bileşen içe aktarma:** `unplugin-vue-components` + `@openvue/auto-import-resolver`. **Dışa aktarılan sembolün adı fork'ta korunmuştur: `PrimeVueResolver`**, `OpenVueResolver` değil.
- **Composable yolları:** `openvue/usetoast`, `openvue/useconfirm`. Yapılandırma: `openvue/config`, tema: `@openvue/themes/aura`.
- **v5'in bileşik `Sidebar` ailesi yoktur.** Navigasyon düz CSS kenar çubuğu + dar ekranda `Drawer` ile kurulur.
- **`sqlx::query!` derleme zamanı makroları kullanılmaz.** Yalnızca `sqlx::query_as::<_, T>()` ve `sqlx::query()` çalışma zamanı sorguları, `#[derive(sqlx::FromRow)]` ile.
- **Mesafe:** `companies.one_way_distance_km` **tek yön yol mesafesidir** ve yalnızca CSV'den gelir ya da elle girilir. Saat tavanı kurallarında kullanılan değer `round_trip_distance_km = one_way_distance_km * 2`'dir ve **saklanmaz**, sorguda türetilir.
- **Mesafe hesaplanmaz.** Haversine veya benzeri kuş uçuşu hesap **hiçbir yerde kullanılmaz**; şehir içinde gerçek araç yol mesafesinden belirgin biçimde kısadır ve saat tavanını yanlış aralığa düşürür. Koordinatlar yalnızca harita gösterimi içindir.
- **Saatler tam sayıdır:** `max_hours`, `max_hours_snapshot`, `awarded_hours` → `INTEGER`. İşletme ders saati 60 dakikadır.
- **Türkçe karakter:** Büyük/küçük harf duyarsız karşılaştırma ve mükerrer tespiti **Rust tarafında** `to_lowercase()` ile yapılır. SQLite'ın `UPPER()`/`LOWER()` fonksiyonları yalnızca ASCII'yi dönüştürür, kullanılmaz.
- **Nominatim hız sınırı: saniyede en fazla 1 istek**, tanımlayıcı `User-Agent` başlığı zorunlu. OSM kullanım koşulları gereğidir; uyulmazsa IP engellenir.
- **Hiçbir hata sessizce yutulmaz.** Coğrafi kodlama başarısızlığı bir hata değil, kaydedilen bir durumdur (`geocode_status = 'failed'`).
- **Her görev commit ile biter.** Commit mesajları `<type>: <açıklama>` biçiminde (`feat`, `fix`, `refactor`, `docs`, `test`, `chore`).

**Spec:** `docs/superpowers/specs/2026-09-18-mesnet-lite-design.md`

---

## Dosya Yapısı — Faz 1'de dokunulan dosyalar

| Dosya | Sorumluluk |
|---|---|
| `src-tauri/src/error.rs` | Tek `AppError` tipi, `serde::Serialize` uygular |
| `src-tauri/src/domain/models.rs` | Depo ve komut katmanının paylaştığı serde tipleri |
| `src-tauri/src/db/mod.rs` | Havuz kurulumu, migration çalıştırma, `AppState` |
| `src-tauri/src/db/companies.rs` | `companies` tablosu CRUD |
| `src-tauri/src/db/students.rs` | `students` tablosu CRUD |
| `src-tauri/src/db/teachers.rs` | `teachers` tablosu CRUD |
| `src-tauri/src/services/csv_import.rs` | JotForm CSV ayrıştırma, saf fonksiyon |
| `src-tauri/src/services/geocoding.rs` | Nominatim istemcisi, hız sınırlayıcı |
| `src-tauri/src/commands/*.rs` | Tauri komut sarmalayıcıları, iş mantığı yok |
| `src-tauri/migrations/0001_initial.sql` | Tüm şema |
| `src-tauri/migrations/0002_seed_hour_rules.sql` | 16 satırlık saat tavanı tablosu |
| `src/i18n/labels.ts` | Türkçe etiket sözlüğü |
| `src/api/*.ts` | Tipli `invoke` sarmalayıcıları |
| `src/components/layout/AppSidebar.vue` | Kenar çubuğu navigasyon kabuğu (geniş ekran `<aside>`, dar ekran `Drawer`) |
| `src/components/map/CompanyMap.vue` | Leaflet haritası |
| `src/views/*.vue` | Ekranlar |

---

### Task 1: Proje iskeleti ve OpenVue kurulumu

**Files:**
- Create: `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/main.ts`, `src/App.vue`
- Create: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Create: `src/i18n/labels.ts`

**Interfaces:**
- Consumes: yok (ilk görev)
- Produces: Çalışan `npm run tauri dev`; otomatik kayıtlı PrimeVue bileşenleri; `src/i18n/labels.ts` içinden `labels` nesnesi

- [x] **Step 1: Tauri 2 + Vue + TypeScript iskeletini oluştur**

Proje dizini zaten var ve git deposu kurulu (`docs/`, `.gitignore`, CSV mevcut).
`npm create tauri-app` boş olmayan dizinde mevcut dosyalara müdahale edebilir, bu yüzden
iskelet önce geçici bir dizine kurulur, sonra yalnızca gereken dosyalar kopyalanır:

```bash
cd /tmp
npm create tauri-app@latest mesnet-scaffold -- --template vue-ts --manager npm --yes

P=/Users/ogretmen/VSCodeProjects/MESNET.Lite
cp -R /tmp/mesnet-scaffold/index.html /tmp/mesnet-scaffold/package.json \
      /tmp/mesnet-scaffold/tsconfig.json /tmp/mesnet-scaffold/tsconfig.node.json \
      /tmp/mesnet-scaffold/vite.config.ts /tmp/mesnet-scaffold/README.md "$P/"
cp -R /tmp/mesnet-scaffold/public /tmp/mesnet-scaffold/src \
      /tmp/mesnet-scaffold/src-tauri /tmp/mesnet-scaffold/.vscode "$P/"
```

Proje `.gitignore` dosyası korunur; iskeletinki kopyalanmaz.

- [ ] **Step 1b: İskeletten gelen `scaffold` adını değiştir**

`npm create tauri-app <ad>` verilen dizin adını crate ve ürün adı olarak yazar.
Geçici dizin adı kullanıldığı için dört yerde düzeltme gerekir:

- `src-tauri/Cargo.toml` → `name = "mesnet-lite"`, `[lib] name = "mesnet_lite_lib"`
- `src-tauri/src/main.rs` → `mesnet_lite_lib::run()`
- `src-tauri/tauri.conf.json` → `productName: "MESNET.Lite"`, `identifier: "ai.alplab.mesnet-lite"`, pencere `title: "MESNET.Lite"`, `width: 1280`, `height: 800`
- `package.json` → `"name": "mesnet-lite"`, `"scripts"` içine `"test": "vitest run"`

Atlanırsa `cargo test` ve `npm run build` çalışır ama üretilen uygulama iskelet adını taşır.

- [x] **Step 2: Paketleri kur**

```bash
npm install openvue@1.0.0 @openvue/themes@1.0.0 primeicons vue-router@4 pinia leaflet
npm install --save-dev @openvue/auto-import-resolver unplugin-vue-components @types/leaflet vitest @vue/test-utils jsdom
```

- [x] **Step 3: `vite.config.ts` içine otomatik bileşen çözümleyiciyi ekle**

```typescript
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import Components from 'unplugin-vue-components/vite'
// Fork export adini korumus: paket @openvue, sembol PrimeVueResolver.
import { PrimeVueResolver } from '@openvue/auto-import-resolver'

// Tauri geliştirme sunucusu sabit port bekler
export default defineConfig({
  plugins: [
    vue(),
    Components({ resolvers: [PrimeVueResolver()] }),
  ],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: { environment: 'jsdom' },
})
```

- [x] **Step 4: `src/main.ts` içinde OpenVue'yu Aura preset'i ile kur**

```typescript
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import OpenVue from 'openvue/config'
import ToastService from 'openvue/toastservice'
import ConfirmationService from 'openvue/confirmationservice'
import Aura from '@openvue/themes/aura'
import 'primeicons/primeicons.css'
import App from './App.vue'
import router from './router'
import 'leaflet/dist/leaflet.css'

const app = createApp(App)
app.use(createPinia())
app.use(router)
// OpenVue, PrimeVue 4.5.5'in MIT lisanslı devamıdır; lisans anahtarı gerektirmez.
app.use(OpenVue, { theme: { preset: Aura } })
app.use(ToastService)
app.use(ConfirmationService)
app.mount('#app')
```

- [x] **Step 5: `index.html` kök font boyutunu 14px'e sabitle**

`<head>` içine ekle:

```html
<style>
  /* OpenVue, PrimeVue 4.5.5 tabanlıdır ve 14px kök font varsayar. */
  html { font-size: 14px; }
  body { margin: 0; }
</style>
```

- [x] **Step 6: `src/i18n/labels.ts` oluştur**

```typescript
// Tüm Türkçe arayüz metinleri burada toplanır; başka dosyada sabit metin yazılmaz.
export const labels = {
  app: { title: 'MESNET.Lite' },
  nav: {
    groupRecords: 'Kayıtlar',
    groupPlanning: 'Planlama',
    groupReports: 'Raporlar',
    groupAdmin: 'Yönetim',
    dashboard: 'Genel Bakış',
    companies: 'İşletmeler',
    students: 'Öğrenciler',
    teachers: 'Öğretmenler',
    allocation: 'Dağıtım',
    availability: 'Müsaitlik Takvimi',
    assignmentSheet: 'Görevlendirme Çizelgesi',
    visitLists: 'Öğretmen Ziyaret Listeleri',
    settings: 'Ayarlar',
    importExport: 'İçe/Dışa Aktarım',
  },
  common: {
    add: 'Ekle', edit: 'Düzenle', delete: 'Sil', save: 'Kaydet',
    cancel: 'Vazgeç', search: 'Ara', confirm: 'Onayla',
    yes: 'Evet', no: 'Hayır', loading: 'Yükleniyor…',
    deleteConfirm: 'Bu kaydı silmek istediğinize emin misiniz?',
    saved: 'Kaydedildi', deleted: 'Silindi', error: 'Hata',
  },
  company: {
    title: 'İşletmeler', name: 'İşletme Adı',
    contactFirstName: 'Yetkili Adı', contactLastName: 'Yetkili Soyadı',
    phone: 'Telefon', email: 'E-posta', address: 'Adres',
    oneWayDistance: 'Tek Yön Mesafe (km)', roundTripDistance: 'Gidiş-Dönüş (km)',
    geocodeStatus: 'Konum Durumu', notes: 'Notlar',
    studentCount: 'Öğrenci Sayısı',
  },
  geocodeStatus: {
    pending: 'Bekliyor', resolved: 'Bulundu',
    failed: 'Bulunamadı', manual: 'Elle Düzeltildi',
  },
  student: {
    title: 'Öğrenciler', firstName: 'Ad', lastName: 'Soyad',
    studentNo: 'Öğrenci No', grade: 'Sınıf', branch: 'Dal',
    company: 'İşletme', submittedAt: 'Başvuru Tarihi',
  },
  teacher: {
    title: 'Öğretmenler', firstName: 'Ad', lastName: 'Soyad',
    registryNo: 'Sicil No', field: 'Alan', branches: 'Dallar',
    employmentType: 'Kadro Tipi', baseHours: 'Aylık Karşılığı Ders',
    maxExtraHours: 'Azami Ek Ders', otherExtraHours: 'Diğer Ek Dersler',
    chiefType: 'Şeflik', chiefHours: 'Şeflik Saati',
    capacity: 'Koordinatörlük Kapasitesi', isActive: 'Aktif',
  },
  chiefType: {
    none: 'Şef Değil',
    workshop_lab: 'Atölye/Laboratuvar Şefi',
    department: 'Bölüm Şefi',
  },
  employmentType: { tenured: 'Kadrolu', contracted: 'Sözleşmeli' },
} as const
```

- [x] **Step 7: Uygulamanın çalıştığını doğrula**

Çalıştır: `npm run tauri dev`
Beklenen: Masaüstü penceresi açılır, hata yok. Pencereyi kapat.

- [x] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: Tauri 2 + Vue 3 + OpenVue iskeleti ve Türkçe etiket sözlüğü"
```

---

### Task 2: Yönlendirme ve kenar çubuğu navigasyon kabuğu

**Files:**
- Create: `src/router/index.ts`, `src/components/layout/AppSidebar.vue`
- Create: `src/views/DashboardView.vue`, `src/views/CompaniesView.vue`, `src/views/StudentsView.vue`, `src/views/TeachersView.vue`, `src/views/SettingsView.vue`, `src/views/ImportExportView.vue`
- Modify: `src/App.vue`
- Test: `src/components/layout/AppSidebar.spec.ts`

**Interfaces:**
- Consumes: `labels` (Task 1)
- Produces: `/`, `/companies`, `/students`, `/teachers`, `/settings`, `/import-export` rotaları; `AppSidebar` bileşeni

- [x] **Step 1: Başarısız testi yaz**

`src/components/layout/AppSidebar.spec.ts`:

```typescript
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import AppSidebar from './AppSidebar.vue'
import { labels } from '../../i18n/labels'

describe('AppSidebar', () => {
  it('tüm ana menü başlıklarını Türkçe olarak gösterir', () => {
    // `RouterLink: true` stub'i slot icerigini BASMAZ; menu etiketleri kaybolur.
    // Ayrica `to` prop olarak bildirildiginde oznitelik olarak dusmez, bu yuzden
    // data-to ile yansitilir.
    const wrapper = mount(AppSidebar, {
      global: {
        stubs: {
          RouterLink: { template: '<a><slot /></a>' },
          RouterView: { template: '<div />' },
        },
      },
    })
    const text = wrapper.text()
    expect(text).toContain(labels.nav.companies)
    expect(text).toContain(labels.nav.students)
    expect(text).toContain(labels.nav.teachers)
    expect(text).toContain(labels.nav.settings)
  })
})
```

- [x] **Step 2: Testi çalıştır, başarısız olduğunu doğrula**

Çalıştır: `npx vitest run src/components/layout/AppSidebar.spec.ts`
Beklenen: FAIL — `Failed to resolve import "./AppSidebar.vue"`

- [x] **Step 3: Yönlendiriciyi oluştur**

`src/router/index.ts`:

```typescript
import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'dashboard', component: () => import('../views/DashboardView.vue') },
    { path: '/companies', name: 'companies', component: () => import('../views/CompaniesView.vue') },
    { path: '/students', name: 'students', component: () => import('../views/StudentsView.vue') },
    { path: '/teachers', name: 'teachers', component: () => import('../views/TeachersView.vue') },
    { path: '/settings', name: 'settings', component: () => import('../views/SettingsView.vue') },
    { path: '/import-export', name: 'importExport', component: () => import('../views/ImportExportView.vue') },
  ],
})

export default router
```

- [x] **Step 4: Altı görünüm dosyasını yer tutucu olarak oluştur**

`src/views/DashboardView.vue`:

```vue
<template>
  <div class="p-4">
    <h1 class="text-2xl font-semibold">{{ labels.nav.dashboard }}</h1>
  </div>
</template>

<script setup lang="ts">
import { labels } from '../i18n/labels'
</script>
```

Diğer beş dosya birebir aynı yapıda; yalnızca başlık ifadesi değişir:

- `src/views/CompaniesView.vue` → `{{ labels.nav.companies }}`
- `src/views/StudentsView.vue` → `{{ labels.nav.students }}`
- `src/views/TeachersView.vue` → `{{ labels.nav.teachers }}`
- `src/views/SettingsView.vue` → `{{ labels.nav.settings }}`
- `src/views/ImportExportView.vue` → `{{ labels.nav.importExport }}`

- [x] **Step 5: `AppSidebar.vue` bileşenini yaz**

OpenVue (PrimeVue 4.5.5 tabanlı) v5'in bileşik `Sidebar` ailesini içermez, bu yüzden
navigasyon elle kurulur:

- Geniş ekranda (`>= 900px`) kalıcı bir `<aside class="sidebar">`; `isCollapsed` durumu
  genişliği `16rem` ↔ `3.5rem` arasında değiştirir ve etiketleri gizler.
- Dar ekranda aynı menü OpenVue `Drawer` bileşeni içinde `position="left"` ile açılır.
- Üst çubuktaki `pi pi-bars` `Button`'ı geniş ekranda daraltır, dar ekranda Drawer'ı açar.
- Menü öğeleri `RouterLink`; aktif rota `.router-link-active` ile vurgulanır.
- Renkler OpenVue tasarım belirteçlerinden okunur (`--p-content-border-color`,
  `--p-content-hover-background`, `--p-highlight-background`, `--p-text-muted-color`);
  sabit renk yazılmaz, böylece tema değişince kenar çubuğu da değişir.
- İkonlar `primeicons` CSS sınıflarıdır: `pi pi-building`, `pi pi-users`, `pi pi-id-card`,
  `pi pi-cog`, `pi pi-file-import`.

Uygulanan bileşen `src/components/layout/AppSidebar.vue` dosyasında birebir mevcuttur.

- [x] **Step 6: `src/App.vue` içeriğini değiştir**

```vue
<template>
  <AppSidebar />
  <Toast />
  <ConfirmDialog />
</template>

<script setup lang="ts">
import AppSidebar from './components/layout/AppSidebar.vue'
</script>
```

- [x] **Step 7: Testi çalıştır, geçtiğini doğrula**

Çalıştır: `npx vitest run src/components/layout/AppSidebar.spec.ts`
Beklenen: PASS

- [x] **Step 8: Uygulamayı çalıştır ve gezinmeyi doğrula**

Çalıştır: `npm run tauri dev`
Beklenen: Sol tarafta Sidebar görünür, ikon moduna daraltılabilir, menü öğelerine tıklayınca sağdaki başlık değişir.

- [x] **Step 9: Commit**

```bash
git add -A
git commit -m "feat: PrimeVue v5 Sidebar navigasyon kabuğu ve yönlendirme"
```

---

### Task 3: Hata tipi, veritabanı havuzu ve şema migration'ları

**Files:**
- Create: `src-tauri/src/error.rs`, `src-tauri/src/db/mod.rs`
- Create: `src-tauri/migrations/0001_initial.sql`, `src-tauri/migrations/0002_seed_hour_rules.sql`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`
- Test: `src-tauri/src/db/mod.rs` içindeki `#[cfg(test)]` modülü

**Interfaces:**
- Consumes: yok
- Produces:
  - `pub enum AppError` — varyantlar: `Database(String)`, `Validation(String)`, `NotFound(String)`, `CsvParse(String)`, `Geocoding(String)`, `Io(String)`
  - `pub type AppResult<T> = Result<T, AppError>`
  - `pub struct AppState { pub pool: sqlx::SqlitePool }`
  - `pub async fn init_pool(db_path: &std::path::Path) -> AppResult<sqlx::SqlitePool>`

- [x] **Step 1: `Cargo.toml` bağımlılıklarını ekle**

`[dependencies]` altına:

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "macros", "migrate"] }
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
tokio = { version = "1", features = ["time"] }
csv = "1.3"
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
serde_json = "1"
```

`[dev-dependencies]` bölümü ekle:

```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [x] **Step 2: Başarısız testi yaz**

`src-tauri/src/db/mod.rs` dosyasını yalnızca bu test modülüyle oluştur:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Havuz kurulumu veritabanı dosyasını oluşturmalı ve migration'ları uygulamalı.
    #[tokio::test]
    async fn init_pool_creates_schema_and_seeds_hour_rules() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        let pool = init_pool(&path).await.unwrap();

        // Seed migration 16 satır kural yazmalı
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM company_hour_rules")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 16);

        // Tüm tablolar oluşmuş olmalı
        for table in [
            "companies", "students", "teachers", "teacher_availability",
            "class_workplace_days", "company_hour_rules", "assignments",
            "assignment_slots", "settings",
        ] {
            let found: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(found, 1, "tablo bulunamadı: {table}");
        }
    }

    /// 0-1 km / 1-2 öğrenci hücresi 2 saat vermeli (seed tablosunun ilk hücresi).
    #[tokio::test]
    async fn seed_first_cell_is_two_hours() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        let hours: i64 = sqlx::query_scalar(
            "SELECT max_hours FROM company_hour_rules
             WHERE min_distance_km = 0.0 AND min_students = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(hours, 2);
    }

    /// 5+ km / 6+ öğrenci hücresi 11 saat vermeli (seed tablosunun son hücresi).
    #[tokio::test]
    async fn seed_last_cell_is_eleven_hours() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        let hours: i64 = sqlx::query_scalar(
            "SELECT max_hours FROM company_hour_rules
             WHERE min_distance_km = 5.0 AND max_distance_km IS NULL
               AND min_students = 6 AND max_students IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(hours, 11);
    }
}
```

- [x] **Step 3: Testi çalıştır, başarısız olduğunu doğrula**

Çalıştır: `cd src-tauri && cargo test db::tests`
Beklenen: FAIL — `cannot find function 'init_pool' in this scope`

- [x] **Step 4: `src-tauri/src/error.rs` yaz**

```rust
/// Uygulamanın tek hata tipi. Tauri komutlarından doğrudan döner.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Veritabanı hatası: {0}")]
    Database(String),

    #[error("Doğrulama hatası: {0}")]
    Validation(String),

    #[error("Kayıt bulunamadı: {0}")]
    NotFound(String),

    #[error("CSV ayrıştırma hatası: {0}")]
    CsvParse(String),

    #[error("Coğrafi kodlama hatası: {0}")]
    Geocoding(String),

    #[error("Dosya hatası: {0}")]
    Io(String),
}

pub type AppResult<T> = Result<T, AppError>;

// Tauri komutları hatayı frontend'e serileştirerek gönderir.
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("Aranan kayıt yok".into()),
            other => AppError::Database(other.to_string()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}
```

- [x] **Step 5: `src-tauri/migrations/0001_initial.sql` yaz**

```sql
-- MESNET.Lite başlangıç şeması.
-- Gün numaralandırması: 1 = Pazartesi … 5 = Cuma
-- Saatler tam saat tamsayısıdır (9 = 09:00-10:00); işletme ders saati 60 dakikadır.

PRAGMA foreign_keys = ON;

-- İşletmeler
CREATE TABLE companies (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL,
    contact_first_name  TEXT    NOT NULL DEFAULT '',
    contact_last_name   TEXT    NOT NULL DEFAULT '',
    phone               TEXT    NOT NULL DEFAULT '',
    email               TEXT    NOT NULL DEFAULT '',
    address_text        TEXT    NOT NULL,
    latitude            REAL,
    longitude           REAL,
    geocode_status      TEXT    NOT NULL DEFAULT 'pending'
                                CHECK (geocode_status IN ('pending','resolved','failed','manual')),
    -- TEK YÖN yol mesafesi. Saat kurallarında iki katı kullanılır; iki katı saklanmaz.
    -- Bu değer koordinatlardan HESAPLANMAZ; CSV'den gelir veya elle girilir.
    one_way_distance_km REAL,
    notes               TEXT    NOT NULL DEFAULT '',
    created_at          TEXT    NOT NULL,
    updated_at          TEXT    NOT NULL
);

-- Öğrenciler
CREATE TABLE students (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    first_name   TEXT    NOT NULL,
    last_name    TEXT    NOT NULL,
    student_no   TEXT,
    grade        TEXT    NOT NULL,
    branch       TEXT    NOT NULL,
    company_id   INTEGER REFERENCES companies(id) ON DELETE SET NULL,
    submitted_at TEXT
);

CREATE INDEX idx_students_company ON students(company_id);

-- Öğretmenler
CREATE TABLE teachers (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    first_name         TEXT    NOT NULL,
    last_name          TEXT    NOT NULL,
    registry_no        TEXT    NOT NULL DEFAULT '',
    field              TEXT    NOT NULL,
    branches           TEXT    NOT NULL DEFAULT '[]',
    employment_type    TEXT    NOT NULL DEFAULT 'tenured'
                               CHECK (employment_type IN ('tenured','contracted')),
    base_hours         INTEGER NOT NULL DEFAULT 20,   -- MADDE 5/1-ç
    max_extra_hours    INTEGER NOT NULL DEFAULT 24,   -- MADDE 6/1-c
    other_extra_hours  INTEGER NOT NULL DEFAULT 0,
    chief_type         TEXT    NOT NULL DEFAULT 'none'
                               CHECK (chief_type IN ('none','workshop_lab','department')),
    is_active          INTEGER NOT NULL DEFAULT 1
);

-- Öğretmenlerin boş saatleri. Satır varsa o saat BOŞ demektir.
CREATE TABLE teacher_availability (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    teacher_id  INTEGER NOT NULL REFERENCES teachers(id) ON DELETE CASCADE,
    day_of_week INTEGER NOT NULL CHECK (day_of_week BETWEEN 1 AND 5),
    hour        INTEGER NOT NULL,
    term        TEXT    NOT NULL,
    UNIQUE (teacher_id, day_of_week, hour, term)
);

-- Sınıfların işletmede bulunduğu günler
CREATE TABLE class_workplace_days (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    grade       TEXT    NOT NULL,
    day_of_week INTEGER NOT NULL CHECK (day_of_week BETWEEN 1 AND 5),
    term        TEXT    NOT NULL,
    UNIQUE (grade, day_of_week, term)
);

-- İşletme saat tavanı kuralları.
-- min_distance_km / max_distance_km GİDİŞ-DÖNÜŞ km'dir (tek yönün iki katı).
CREATE TABLE company_hour_rules (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    min_distance_km REAL    NOT NULL,
    max_distance_km REAL,               -- NULL = üst sınırsız
    min_students    INTEGER NOT NULL,
    max_students    INTEGER,            -- NULL = üst sınırsız
    max_hours       INTEGER NOT NULL
);

-- Atamalar. Bir işletme dönem başına tek koordinatöre bağlanır.
CREATE TABLE assignments (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    teacher_id         INTEGER NOT NULL REFERENCES teachers(id) ON DELETE CASCADE,
    company_id         INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    awarded_hours      INTEGER NOT NULL,
    max_hours_snapshot INTEGER NOT NULL,
    is_forced          INTEGER NOT NULL DEFAULT 0,
    force_reason       TEXT,
    term               TEXT    NOT NULL,
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL,
    UNIQUE (company_id, term)
);

-- Atamanın haftalık ziyaret dilimleri. Her satır bir saattir.
CREATE TABLE assignment_slots (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    assignment_id INTEGER NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    day_of_week   INTEGER NOT NULL CHECK (day_of_week BETWEEN 1 AND 5),
    hour          INTEGER NOT NULL,
    UNIQUE (assignment_id, day_of_week, hour)
);

-- Anahtar/değer ayarlar
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO settings (key, value) VALUES
    ('school_name',              'Atatürk Mesleki ve Teknik Anadolu Lisesi'),
    ('school_latitude',          ''),
    ('school_longitude',         ''),
    ('institution_type',         'other'),
    ('is_metropolitan_district', 'true'),
    ('active_term',              '2026-2027/1'),
    ('day_start_hour',           '8'),
    ('day_end_hour',             '17'),
    ('branch_weekly_hours',      '{}'),
    ('branch_group_counts',      '{}');
```

- [x] **Step 6: `src-tauri/migrations/0002_seed_hour_rules.sql` yaz**

```sql
-- İşletme saat tavanı başlangıç tablosu.
-- SATIRLAR GİDİŞ-DÖNÜŞ km'dir (tek yönün iki katı).
--
--  gidiş-dönüş km | 1-2 öğr. | 3-4 öğr. | 5-6 öğr. | 6+ öğr.
--  0 - 1          |    2     |    3     |    4     |   5
--  1 - 3          |    4     |    5     |    6     |   7
--  3 - 5          |    6     |    7     |    8     |   9
--  5 +            |    8     |    9     |   10     |  11
--
-- Not: 5-6 ve 6+ sütunları 6 öğrencide çakışır. "En dar kural" sıralaması
-- (spec §6) 6 öğrenciyi 5-6 sütununa yerleştirir. Belirlenimcidir.

INSERT INTO company_hour_rules
    (min_distance_km, max_distance_km, min_students, max_students, max_hours)
VALUES
    (0.0, 1.0, 1, 2,    2),
    (0.0, 1.0, 3, 4,    3),
    (0.0, 1.0, 5, 6,    4),
    (0.0, 1.0, 6, NULL, 5),

    (1.0, 3.0, 1, 2,    4),
    (1.0, 3.0, 3, 4,    5),
    (1.0, 3.0, 5, 6,    6),
    (1.0, 3.0, 6, NULL, 7),

    (3.0, 5.0, 1, 2,    6),
    (3.0, 5.0, 3, 4,    7),
    (3.0, 5.0, 5, 6,    8),
    (3.0, 5.0, 6, NULL, 9),

    (5.0, NULL, 1, 2,    8),
    (5.0, NULL, 3, 4,    9),
    (5.0, NULL, 5, 6,   10),
    (5.0, NULL, 6, NULL, 11);
```

- [x] **Step 7: `src-tauri/src/db/mod.rs` içeriğini tamamla**

Step 2'de yazdığın test modülünün **üstüne** ekle:

```rust
use crate::error::{AppError, AppResult};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;

pub mod companies;
pub mod students;
pub mod teachers;

/// Tauri yönetilen durumu. Komutlar havuza buradan erişir.
pub struct AppState {
    pub pool: SqlitePool,
}

/// Veritabanı havuzunu kurar, dosya yoksa oluşturur ve migration'ları uygular.
pub async fn init_pool(db_path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(format!("Havuz açılamadı: {e}")))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(format!("Migration başarısız: {e}")))?;

    Ok(pool)
}
```

Ardından `src-tauri/src/db/companies.rs`, `src-tauri/src/db/students.rs`, `src-tauri/src/db/teachers.rs` dosyalarını **boş** oluştur — aksi hâlde `pub mod` bildirimleri derlenmez. Bu dosyalar Task 5, 12 ve 13'te doldurulacak.

- [x] **Step 8: `src-tauri/src/lib.rs` içinde modülleri bildir ve havuzu kur**

```rust
mod commands;
mod db;
mod domain;
mod error;
mod services;

use db::{init_pool, AppState};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Veritabanı platform-doğru uygulama veri dizininde tutulur.
            let dir = app.path().app_data_dir()?;
            let db_path = dir.join("mesnet-lite.db");

            let pool = tauri::async_runtime::block_on(init_pool(&db_path))
                .map_err(|e| format!("Veritabanı açılamadı: {e}"))?;

            app.manage(AppState { pool });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Uygulama başlatılamadı");
}
```

`src-tauri/src/commands/mod.rs`, `src-tauri/src/domain/mod.rs`, `src-tauri/src/services/mod.rs` dosyalarını boş oluştur.

- [x] **Step 9: Testi çalıştır, geçtiğini doğrula**

Çalıştır: `cd src-tauri && cargo test db::tests`
Beklenen: PASS — üç test de geçer

- [x] **Step 10: Commit**

```bash
git add -A
git commit -m "feat: SQLite şeması, saat tavanı seed tablosu ve AppError tipi"
```

---

### Task 4: Alan modelleri (`domain/models.rs`)

**Files:**
- Create: `src-tauri/src/domain/models.rs`
- Modify: `src-tauri/src/domain/mod.rs`

**Interfaces:**
- Consumes: yok
- Produces: `Company`, `NewCompany`, `Student`, `NewStudent`, `Teacher`, `NewTeacher`, `GeocodeStatus`, `ChiefType`, `EmploymentType`; `ChiefType::weekly_hours() -> i64`; `Company::round_trip_distance_km() -> Option<f64>`

- [x] **Step 1: Başarısız testi yaz**

`src-tauri/src/domain/models.rs` dosyasını yalnızca bu test modülüyle oluştur:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// MADDE 6/4: bölüm şefi haftada 10, atölye ve laboratuvar şefi haftada 6 saat.
    #[test]
    fn chief_type_weekly_hours_follows_madde_6_4() {
        assert_eq!(ChiefType::Department.weekly_hours(), 10);
        assert_eq!(ChiefType::WorkshopLab.weekly_hours(), 6);
        assert_eq!(ChiefType::None.weekly_hours(), 0);
    }

    /// Saat tavanı kuralları gidiş-dönüş mesafe kullanır: tek yönün iki katı.
    #[test]
    fn round_trip_distance_doubles_one_way() {
        let company = Company {
            id: 1,
            name: "Test İşletme A".into(),
            contact_first_name: String::new(),
            contact_last_name: String::new(),
            phone: String::new(),
            email: String::new(),
            address_text: "Test adres".into(),
            latitude: None,
            longitude: None,
            geocode_status: "pending".into(),
            one_way_distance_km: Some(6.8),
            notes: String::new(),
            created_at: "2026-09-18T00:00:00Z".into(),
            updated_at: "2026-09-18T00:00:00Z".into(),
        };
        assert_eq!(company.round_trip_distance_km(), Some(13.6));
    }

    /// Mesafe bilinmiyorsa gidiş-dönüş de bilinmez; sıfır varsayılmaz.
    #[test]
    fn round_trip_distance_is_none_when_one_way_missing() {
        let company = Company {
            id: 2,
            name: "Test İşletme B".into(),
            contact_first_name: String::new(),
            contact_last_name: String::new(),
            phone: String::new(),
            email: String::new(),
            address_text: "Test adres".into(),
            latitude: None,
            longitude: None,
            geocode_status: "pending".into(),
            one_way_distance_km: None,
            notes: String::new(),
            created_at: "2026-09-18T00:00:00Z".into(),
            updated_at: "2026-09-18T00:00:00Z".into(),
        };
        assert_eq!(company.round_trip_distance_km(), None);
    }
}
```

- [x] **Step 2: Testi çalıştır, başarısız olduğunu doğrula**

Çalıştır: `cd src-tauri && cargo test domain::models`
Beklenen: FAIL — `cannot find type 'ChiefType' in this scope`

- [x] **Step 3: Modelleri yaz**

Test modülünün **üstüne** ekle:

```rust
use serde::{Deserialize, Serialize};

/// İşletmenin konum bilgisinin durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GeocodeStatus {
    Pending,
    Resolved,
    Failed,
    Manual,
}

/// Şeflik görevi. Saat değeri buradan türetilir, veritabanında saklanmaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChiefType {
    None,
    WorkshopLab,
    Department,
}

impl ChiefType {
    /// MADDE 6/4: bölüm şefleri için haftada 10, atölye ve laboratuvar
    /// şefleri için haftada 6 saat. Bu saatler azamî ek ders tavanının
    /// İÇİNDE verilir, üstüne eklenmez.
    pub fn weekly_hours(self) -> i64 {
        match self {
            ChiefType::None => 0,
            ChiefType::WorkshopLab => 6,
            ChiefType::Department => 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmploymentType {
    Tenured,
    Contracted,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Company {
    pub id: i64,
    pub name: String,
    pub contact_first_name: String,
    pub contact_last_name: String,
    pub phone: String,
    pub email: String,
    pub address_text: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub geocode_status: String,
    /// TEK YÖN yol mesafesi. Koordinatlardan hesaplanmaz; CSV'den gelir
    /// veya elle girilir. Saat kuralları iki katını kullanır.
    pub one_way_distance_km: Option<f64>,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Company {
    /// Saat tavanı kurallarında kullanılan gidiş-dönüş mesafesi.
    pub fn round_trip_distance_km(&self) -> Option<f64> {
        self.one_way_distance_km.map(|km| km * 2.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCompany {
    pub name: String,
    pub contact_first_name: String,
    pub contact_last_name: String,
    pub phone: String,
    pub email: String,
    pub address_text: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub one_way_distance_km: Option<f64>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Student {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub student_no: Option<String>,
    pub grade: String,
    pub branch: String,
    pub company_id: Option<i64>,
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewStudent {
    pub first_name: String,
    pub last_name: String,
    pub student_no: Option<String>,
    pub grade: String,
    pub branch: String,
    pub company_id: Option<i64>,
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Teacher {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub registry_no: String,
    pub field: String,
    /// JSON dizi olarak saklanır, ör. ["Elektronik Haberleşme"]
    pub branches: String,
    pub employment_type: String,
    pub base_hours: i64,
    pub max_extra_hours: i64,
    pub other_extra_hours: i64,
    pub chief_type: String,
    pub is_active: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTeacher {
    pub first_name: String,
    pub last_name: String,
    pub registry_no: String,
    pub field: String,
    pub branches: Vec<String>,
    pub employment_type: String,
    pub base_hours: i64,
    pub max_extra_hours: i64,
    pub other_extra_hours: i64,
    pub chief_type: String,
    pub is_active: bool,
}
```

- [x] **Step 4: `src-tauri/src/domain/mod.rs` içinde modülü bildir**

```rust
pub mod models;
```

- [x] **Step 5: Testi çalıştır, geçtiğini doğrula**

Çalıştır: `cd src-tauri && cargo test domain::models`
Beklenen: PASS — üç test de geçer

- [x] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: alan modelleri, şeflik saati ve gidiş-dönüş mesafe türetmesi"
```

---

### Task 5: `db/companies.rs` deposu

**Files:**
- Modify: `src-tauri/src/db/companies.rs` (Task 3'te boş oluşturuldu)
- Test: aynı dosyadaki `#[cfg(test)]` modülü

**Interfaces:**
- Consumes: `Company`, `NewCompany` (Task 4), `AppResult` (Task 3)
- Produces:
  - `pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Company>>`
  - `pub async fn get(pool: &SqlitePool, id: i64) -> AppResult<Company>`
  - `pub async fn create(pool: &SqlitePool, input: &NewCompany) -> AppResult<Company>`
  - `pub async fn update(pool: &SqlitePool, id: i64, input: &NewCompany) -> AppResult<Company>`
  - `pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()>`
  - `pub async fn set_location(pool: &SqlitePool, id: i64, latitude: f64, longitude: f64, status: &str) -> AppResult<Company>`
  - `pub async fn find_by_normalized_name(pool: &SqlitePool, name: &str) -> AppResult<Option<Company>>`
  - `pub fn normalize_name(name: &str) -> String`

Kod içeriği uygulanan dosyada birebir mevcuttur (`src-tauri/src/db/companies.rs`).
Testler: create/get, sıralı list, update, delete, set_location, `normalize_name` Unicode
davranışı ve `find_by_normalized_name` büyük/küçük harf + boşluk toleransı.

`normalize_name` testindeki beklenti kasıtlıdır: Rust'ın `to_lowercase()` fonksiyonu
`İ` (U+0130) harfini iki kod noktasına (`i` + U+0307) çevirir. Test bu gerçek davranışı
sabitler; karşılaştırmanın iki tarafı da aynı fonksiyondan geçtiği için sorun değildir.

- [ ] **Step 1: Testleri yaz** — yedi test (yukarıdaki liste)
- [ ] **Step 2: `cargo test db::companies` — FAIL bekle**
- [ ] **Step 3: Depoyu yaz**
- [ ] **Step 4: `cargo test db::companies` — PASS bekle**
- [ ] **Step 5: Commit** — `feat: işletme deposu ve Unicode-doğru mükerrer tespiti`

---

### Task 6: İşletme komutları ve tipli frontend API

**Files:**
- Create: `src-tauri/src/commands/company_commands.rs`, `src/api/client.ts`, `src/api/companies.ts`, `src/types/models.ts`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `db::companies` (Task 5), `AppState` (Task 3)
- Produces: Tauri komutları `list_companies`, `get_company`, `create_company`,
  `update_company`, `delete_company`, `set_company_location`; TypeScript `Company`,
  `NewCompany` arayüzleri ve `companiesApi` nesnesi

Komut katmanı incedir: yalnızca sınır doğrulaması yapar, iş mantığı `db/` ve `domain/`
içindedir. Doğrulama kuralları: ad boş olamaz, adres boş olamaz, mesafe negatif olamaz,
enlem -90..90, boylam -180..180.

- [ ] **Step 1: Komut katmanını ve dört doğrulama testini yaz**
- [ ] **Step 2: Komutları `invoke_handler` içinde kaydet**
- [ ] **Step 3: `cargo test` — PASS bekle**
- [ ] **Step 4: `src/types/models.ts` yaz** (serde camelCase karşılığı)
- [ ] **Step 5: `src/api/client.ts` ve `src/api/companies.ts` yaz**
- [ ] **Step 6: `npm run build` — tip hatası olmamalı**
- [ ] **Step 7: Commit** — `feat: işletme Tauri komutları, sınır doğrulaması ve tipli frontend API`

---

### Task 7: `CompaniesView` — DataTable tam CRUD

**Files:**
- Modify: `src/views/CompaniesView.vue`, `src/i18n/labels.ts`
- Create: `src/components/company/CompanyFormDialog.vue`

**Interfaces:**
- Consumes: `companiesApi` (Task 6), `labels` (Task 1)
- Produces: İşletme listeleme / ekleme / düzenleme / silme ekranı

Tablo sütunları: ad (sıralanabilir), adres, telefon, tek yön mesafe, **gidiş-dönüş
mesafe (türetilmiş, iki katı)**, konum durumu (renkli `Tag`), satır işlemleri.
Global arama ad ve adres üzerinde çalışır. Silme `useConfirm` ile onaylanır.
Hata `useToast` ile Türkçe gösterilir, sessizce yutulmaz.

- [ ] **Step 1: `CompanyFormDialog.vue` yaz**
- [ ] **Step 2: Etiket sözlüğüne `distanceHint`, `empty`, `searchPlaceholder` ekle**
- [ ] **Step 3: `CompaniesView.vue` yaz**
- [ ] **Step 4: `npm run build` — tip hatası olmamalı**
- [ ] **Step 5: Uygulamada elle doğrula** — Ekle, listede gör, Düzenle, Sil (onay diyaloğu), gidiş-dönüş sütunu tek yönün iki katı mı
- [ ] **Step 6: Commit** — `feat: işletme CRUD ekranı, filtre ve gidiş-dönüş sütunu`

---

## Kalan görevler

Bu plan dosyası sonraki turlarda aşağıdaki görevlerle tamamlanacaktır. Her biri aynı yapıda (Files / Interfaces / TDD adımları / commit) yazılacaktır.

- **Task 5:** `db/companies.rs` deposu ve entegrasyon testleri
- **Task 6:** `commands/company_commands.rs` ve `src/api/companies.ts`
- **Task 7:** `CompaniesView` — PrimeVue `DataTable` tam CRUD
- **Task 8:** `services/csv_import.rs` — üç satırlı `İşletmenizi bulun` alanının ayrıştırılması ve Unicode-doğru mükerrer tespiti
- **Task 9:** CSV içe aktarma sihirbazı (önizleme → mükerrer çözümü → içe aktarma)
- **Task 10:** `services/geocoding.rs` — Nominatim istemcisi ve saniyede 1 istek hız sınırlayıcı
- **Task 11:** `CompanyMap.vue` — Leaflet haritası ve elle konum düzeltme
- **Task 12:** `db/students.rs`, öğrenci komutları, `StudentsView`
- **Task 13:** `db/teachers.rs`, öğretmen komutları, `TeachersView`
