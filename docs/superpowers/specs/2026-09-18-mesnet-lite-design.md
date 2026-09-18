# MESNET.Lite — Tasarım Dokümanı

**Tarih:** 2026-09-18
**Durum:** Onay bekliyor
**Kapsam:** İşletmelerde mesleki eğitim koordinatörlük dağıtımı ve ek ders saati takdir uygulaması

---

## 1. Amaç

Atatürk Mesleki ve Teknik Anadolu Lisesi Elektrik-Elektronik Teknolojisi Alanı için:

- İşletmelerde mesleki eğitim yapan öğrencilerin ve işletmelerin kalıcı kaydını tutmak
- İşletmeleri koordinatör öğretmenlere mevzuata uygun şekilde dağıtmak
- Her işletme için verilebilecek **azami** haftalık ek ders saatini hesaplamak, gerçek saati kullanıcının **takdirine** bırakmak
- Atamaları öğretmenin boş saatlerine ve öğrencinin işletmede bulunduğu günlere yerleştirmek
- Müdürlük ve İl/İlçe MEM onayına gidecek görevlendirme çizelgesini üretmek

**Kapsam dışı:** Ek ders **ücreti** (TL) hesabı yapılmaz. Yalnızca **saat** hesaplanır.

---

## 2. Dil ve adlandırma kuralı

| Ne | Dil |
|---|---|
| Dosya ve dizin adları | İngilizce |
| Tablo, sütun, tip, fonksiyon, değişken, enum değerleri | İngilizce |
| Kod içi yorumlar | Türkçe |
| Dokümantasyon | Türkçe |
| Arayüz metinleri, etiketler, hata mesajları | Türkçe |

Arayüz etiketleri `src/i18n/labels.ts` içinde tek sözlükte toplanır. Veritabanına Türkçe metin yalnızca **kullanıcı verisi** olarak girer (işletme adı, öğrenci adı); enum veya anahtar olarak asla girmez.

**Türkçe karakter uyarısı:** SQLite'ın yerleşik `UPPER()` / `LOWER()` fonksiyonları yalnızca ASCII'yi dönüştürür (`'işletme'` → `'IşLETME'`). Büyük/küçük harf duyarsız karşılaştırma ve mükerrer tespiti **Rust tarafında** Unicode-doğru `to_lowercase()` ile yapılır, SQL'de değil.

---

## 3. Mevzuat temeli

Tüm hesaplar aşağıdaki hükümlere dayanır. Değerler sabit kodlanmaz; ayarlardan seçilen okul tipine göre tablodan okunur.

### 3.1 Millî Eğitim Bakanlığı Yönetici ve Öğretmenlerinin Ders ve Ek Ders Saatlerine İlişkin Karar (2006/11350 BKK)

**MADDE 5/1** — aylık karşılığı ders yükümlülüğü:

- (ç) Atölye ve laboratuvar öğretmenleri **haftada 20 saat**

**MADDE 6/1** — azamî ek ders görevi:

- (c) Atölye ve laboratuvar öğretmenlerine **20 saati zorunlu olmak üzere haftada 24 saate** kadar

**MADDE 6/4** — şeflik (verbatim):

> *"Birinci fıkranın (c) bendinde belirtilen atölye ve laboratuvar öğretmenlerinden kendilerine bölüm, atölye ve laboratuvar şefliği görevi verilenlerin, görev yaptıkları eğitim kurumunun bölüm, atölye ve laboratuvarlarındaki çalışmaların planlanması, tezgah, makine, araç ve gerecin sağlanması, bakımı, onarımı ve öğretime hazır halde bulundurulması amacıyla yaptıkları çalışmaların **bölüm şefleri için haftada 10, atölye ve laboratuvar şefleri için ise haftada 6 saati** ek ders görevi sayılır. Bu dersler ders dağıtım çizelgesinde "Planlama ve Bakım-Onarım Görevi" adıyla gösterilir ve **haftada azamî okutabilecekleri ek ders saatleri içinde verilir**."*

Son cümle belirleyicidir: şeflik saati ayrı bir bütçe değil, azamî ek ders tavanının **içindedir**.

**MADDE 15** — işletmelerde meslek eğitimi (verbatim; 13/03/2017 tarihli ve 2017/10010 sayılı BKK ile, 29/12/2021 tarihli ve 5029 sayılı CK ile değişik):

> *"(1) İşletmelerde meslek eğitimi yapılan okul ve kurumlarda görevli yönetici ve öğretmenlerin öğrenci, çırak ve aday çırakların işyerindeki uygulamalı eğitimini izlemek, programa uygunluğunu ve sistemin iş yerindeki işlerliğini sağlamak, meslekî rehberlikte bulunmak üzere yaptıkları bu görevler ek ders görevi sayılır.*
>
> *(2) Bu dersler, ders dağıtım çizelgelerinde "işletmelerde meslek eğitimi" adıyla gösterilir ve işletmelerin okul ve kuruma uzaklığı, öğrenci, çırak ve aday çırak sayısı gibi kıstaslar esas alınarak okul ve kurum müdürlüğünce hazırlanacak ve millî eğitim müdürlüğünce onaylanacak programlara göre haftada;*
> *a) Meslekî eğitim merkezlerinde; 1) Büyükşehir belediyesi sınırları içindeki ilçelerde 24 saati, 2) Diğer il ve ilçelerde 18 saati,*
> *b) Diğer okul ve kurumlarda; 1) Büyükşehir belediyesi sınırları içindeki ilçelerde 20 saati, 2) Diğer il ve ilçelerde 16 saati,*
> *geçmemek üzere okutabilecekleri azamî ek ders saatleri kapsamında verilir."*

**MADDE 30** — ek ders birim ücreti 657 sayılı Kanunun 176 ncı maddesine göre ödenir. *Bu uygulama ücret hesabı yapmaz; madde yalnızca kapsam sınırını belgelemek için kaydedilmiştir.*

### 3.2 Millî Eğitim Bakanlığı Ortaöğretim Kurumları Yönetmeliği, MADDE 88

- *"Aynı işletmede aynı alanda mesleki eğitim gören **15 öğrenciye kadar bir koordinatör öğretmen** görevlendirilir"*
- Atölye ve laboratuvar öğretmenleri arasından görevlendirilir; yetersizse *"bu alana yakın alan öğretmenlerine öncelik vermek üzere"*
- Dağıtım kıstasları: *"işletmelerin okula uzaklığı, ulaşım durumu, işletme sayısı, işletmeler arası uzaklık ve işletmedeki öğrenci sayısıyla bunlarla ilgili iş ve işlemlerde harcanılacak zaman"*
- *"Bu kapsamda bir öğretmene **aynı gün için 8 saatten fazla** ek ders görevi verilmez"*

### 3.3 Ders saati tanımı

Mesleki ve teknik ortaöğretim programlarında **işletmelerde yapılan ders saati 60 dakika** üzerinden değerlendirilir. Bu nedenle zamanlama modelinde bir "saat" bir saat dilimidir (ör. 09:00–10:00), okul içi 40 dakikalık ders saati değildir.

### 3.4 Bu okul için sonuçlanan değerler

Atatürk MTAL → `institution_type = other` (meslekî eğitim merkezi değil); Toroslar/Mersin → `is_metropolitan_district = true`.

⇒ **MADDE 15/2-b-1 uyarınca haftalık koordinatörlük tavanı 20 saat.**

---

## 4. Veri kaynağı

`Atatürk_MTAL_EETA_İşletme_Kayıt2026-09-17_03_04_53.csv` — JotForm dışa aktarımı, UTF-8 BOM'lu, 12 sütun, 32 kayıt.

| # | Sütun | Hedef alan |
|---|---|---|
| 0 | `Submission Date` | `students.submitted_at` |
| 1 | `Ad` | `students.first_name` |
| 2 | `Soyad` | `students.last_name` |
| 3 | `İşletme Adı` | `companies.name` |
| 4 | `Ad` | `companies.contact_first_name` |
| 5 | `Soyad` | `companies.contact_last_name` |
| 6 | `Telefon Numarası` | `companies.phone` |
| 7 | `İşletmenizi bulun` | ayrıştırılır — aşağıya bakınız |
| 8 | `E-posta` | `companies.email` |
| 9 | `DAL Bİlgisi Seçiniz` | `students.branch` |
| 10 | `Öğrenci No` | `students.student_no` |
| 11 | `Sınıf Seçiniz` | `students.grade` |

Tarih formatı: CSV'de `Sep 11, 2026` biçiminde; veritabanına ISO-8601 (`2026-09-11`) olarak dönüştürülerek yazılır.

**Sütun 7 ayrıştırma** — üç satırlı tek hücre, yapı örneği:

```
Result: <okulun adı ve adresi>
Distance: 6.8 km
Address: <işletmenin gerçek adresi>
```

- `Result:` satırı **okulun** adresidir, işletmenin değil — yok sayılır
- `Distance:` → `companies.one_way_distance_km` (`6.8 km` → `6.8`) — **tek yön** mesafedir
- `Address:` → `companies.address_text`; **coğrafi kodlamaya giden metin budur**

**Gidiş-dönüş dönüşümü:** CSV'deki `Distance:` değeri okuldan işletmeye **tek yön** mesafedir. Koordinatörlük ziyareti gidiş ve dönüş içerdiğinden saat tavanı kurallarında kullanılan mesafe iki katıdır:

```
round_trip_distance_km = one_way_distance_km × 2
```

Ham değer `one_way_distance_km` olarak saklanır; iki katı **saklanmaz**, kural sorgusunda türetilir. Böylece kaynak veri tek ve tartışmasız kalır.

**Veri kalitesi (doğrulanmış):** 32/32 satırda `Distance:` ve `Address:` ayrıştırılabiliyor · `E-posta` 32/32 boş · `Öğrenci No` 1 satırda boş · Enlem/boylam **yok**.

**Dağılım:** 32 öğrenci, 28 tekil işletme (4 işletme 2 öğrencili) · Dal: Elektronik Haberleşme 16, Endüstriyel Bakım Onarım 9, Elektrik Tesisatları ve Pano Montörlüğü 7 · Sınıf: 12/C 16, 12/D 16.

---

## 5. Teknoloji yığını

| Katman | Seçim | Gerekçe |
|---|---|---|
| Kabuk | Tauri 2 | Masaüstü, Rust backend, küçük ikili |
| Backend | Rust | Hesap mantığı derleme zamanı güvenceli |
| Veritabanı | SQLite + `sqlx` | Gömülü, tek dosya, `sqlx::migrate!` ile şema sürümleme |
| Frontend | Vue 3 + TypeScript + Vite | |
| UI kütüphanesi | **OpenVue 1.0.0** + `@openvue/themes/aura` | PrimeVue 4.5.5'in MIT lisanslı devamı; lisans anahtarı gerektirmez |
| Harita | Leaflet + OpenStreetMap raster tile | API anahtarı gerektirmez |
| Durum yönetimi | Pinia | |
| Excel çıktısı | `rust_xlsxwriter` | |
| PDF çıktısı | Webview yazdırma (`@media print` + `window.print()`) | Ek bağımlılık yok, Türkçe font sorunu yok |

### 5.1 UI kütüphanesi kararı — neden OpenVue

Proje önce **PrimeVue v5** ile kuruldu ve çalışmadı: v5, PrimeUI çatısı altında dual
Community/Commercial lisans modeline geçmiş ve **çalışma zamanında lisans anahtarı
doğruluyor**. Anahtarsız çalıştırıldığında uygulama tüm stillerini kaybetti ve pencerede
`Invalid PrimeUI License` rozeti göründü. Derleme ve testler yeşil olduğu için bu ancak
uygulamanın ekran görüntüsüne bakınca fark edildi.

**OpenVue 1.0.0** seçildi: PrimeVue **4.5.5** forku — *"the last release published under an
open source license"* — MIT lisanslı, *"no paid tiers or locked features"*, lisans anahtarı
istemiyor. API, temalar ve pass-through PrimeVue v4 ile aynı.

Sonuçları:

- **v5'in bileşik `Sidebar` ailesi yoktur** (`SidebarLayout`, `SidebarAside`,
  `SidebarMenuButton` …). Navigasyon düz CSS kenar çubuğu olarak yazılır; dar ekranda
  `Drawer` kullanılır, ikon moduna daraltma elle yapılır.
- Kök font boyutu **14px**'tir (v5 16px varsayıyordu).
- Otomatik içe aktarma paketi `@openvue/auto-import-resolver`, ancak **dışa aktardığı
  sembolün adı fork'ta korunmuştur: `PrimeVueResolver`** (`OpenVueResolver` değil).
- Composable'lar `openvue/usetoast`, `openvue/useconfirm` yolundan gelir.
- İkonlar `primeicons` paketinden CSS sınıflarıyla kullanılır (`pi pi-building` gibi).
- Zamanlama/takvim için hazır bileşen yoktur; müsaitlik ızgarası kendi
  `AvailabilityGrid.vue` bileşenimizle yazılır.

**Sorgu stili:** `sqlx::query!` derleme zamanı makroları **kullanılmaz**; bunlar derleme sırasında canlı bir veritabanı veya `cargo sqlx prepare` ile üretilmiş önbellek gerektirir ve kurulum sürtünmesini artırır. Bunun yerine `sqlx::query_as::<_, T>()` çalışma zamanı sorguları ve `#[derive(sqlx::FromRow)]` kullanılır. Sorgu doğruluğu entegrasyon testleriyle güvence altına alınır (§16).

### 5.2 Veritabanı konumu

`app_data_dir()/mesnet-lite.db` — Tauri'nin platform-doğru veri dizini (macOS: `~/Library/Application Support/`). Tek kullanıcılı, çevrimdışı, kalıcı.

---

## 6. Veri modeli

Gün numaralandırması: `1 = Pazartesi … 5 = Cuma`. Saatler tam saat tamsayısı olarak saklanır (ör. `9` = 09:00–10:00 dilimi), çünkü işletme ders saati 60 dakikadır (§3.3).

### `companies` — işletmeler

| Sütun | Tip | Not |
|---|---|---|
| `id` | INTEGER PK | |
| `name` | TEXT NOT NULL | |
| `contact_first_name`, `contact_last_name` | TEXT | işletme yetkilisi |
| `phone`, `email` | TEXT | |
| `address_text` | TEXT NOT NULL | CSV `Address:` satırı |
| `latitude`, `longitude` | REAL NULL | |
| `geocode_status` | TEXT | `pending` / `resolved` / `failed` / `manual` |
| `one_way_distance_km` | REAL NULL | CSV `Distance:` satırı — **tek yön yol mesafesi**; yalnızca CSV'den veya elle girilir |
| `notes` | TEXT | |
| `created_at`, `updated_at` | TEXT | ISO-8601 |

### `students` — öğrenciler

| Sütun | Tip | Not |
|---|---|---|
| `id` | INTEGER PK | |
| `first_name`, `last_name` | TEXT NOT NULL | |
| `student_no` | TEXT NULL | CSV'de eksik olabilir |
| `grade` | TEXT NOT NULL | `12/C`, `12/D` |
| `branch` | TEXT NOT NULL | dal |
| `company_id` | INTEGER FK → `companies` | |
| `submitted_at` | TEXT | ISO-8601 |

### `teachers` — öğretmenler

| Sütun | Tip | Varsayılan | Not |
|---|---|---|---|
| `id` | INTEGER PK | | |
| `first_name`, `last_name` | TEXT NOT NULL | | |
| `registry_no` | TEXT | | kurum sicil no |
| `field` | TEXT NOT NULL | | alan |
| `branches` | TEXT (JSON dizi) | `[]` | verebileceği dallar |
| `employment_type` | TEXT | `tenured` | `tenured` / `contracted` |
| `base_hours` | INTEGER | `20` | MADDE 5/1-ç, aylık karşılığı |
| `max_extra_hours` | INTEGER | `24` | MADDE 6/1-c, azamî ek ders |
| `other_extra_hours` | INTEGER | `0` | koordinatörlük dışı ek dersler |
| `chief_type` | TEXT | `none` | `none` / `workshop_lab` / `department` |
| `is_active` | INTEGER | `1` | |

`chief_type` saatin kendisini tutmaz; saat mevzuattan türetilir (§7.1). Rakamı saklamak, mevzuat değişiminde tüm satırların güncellenmesini gerektirirdi.

### `teacher_availability` — öğretmenlerin boş saatleri

| Sütun | Tip | Not |
|---|---|---|
| `id` | INTEGER PK | |
| `teacher_id` | INTEGER FK → `teachers` ON DELETE CASCADE | |
| `day_of_week` | INTEGER | 1–5 |
| `hour` | INTEGER | `day_start_hour` … `day_end_hour` |
| `term` | TEXT | dönem |

**UNIQUE(`teacher_id`, `day_of_week`, `hour`, `term`)**

Bir satır = o öğretmenin o gün o saatte **boş** olduğu. Satır yoksa dolu kabul edilir. Arayüzde gün × saat ızgarasında hücre tıklayarak veya sürükleyerek işaretlenir.

### `class_workplace_days` — sınıfların işletme günleri

| Sütun | Tip | Not |
|---|---|---|
| `id` | INTEGER PK | |
| `grade` | TEXT NOT NULL | `12/C` |
| `day_of_week` | INTEGER | 1–5 |
| `term` | TEXT | |

**UNIQUE(`grade`, `day_of_week`, `term`)**

Bir satır = o sınıfın o gün işletmede olduğu. Farklı sınıflar farklı günlerde gider; bu tablo öğrencinin işletmede bulunduğu günleri belirler.

### `company_hour_rules` — işletme saat tavanı kuralları

| Sütun | Tip | Not |
|---|---|---|
| `id` | INTEGER PK | |
| `min_distance_km` | REAL NOT NULL | dahil — **gidiş-dönüş** km |
| `max_distance_km` | REAL NULL | hariç; `NULL` = üst sınırsız |
| `min_students` | INTEGER NOT NULL | dahil |
| `max_students` | INTEGER NULL | dahil; `NULL` = üst sınırsız |
| `max_hours` | INTEGER NOT NULL | tam saat |

Aralık tanımı: `min_distance_km <= round_trip_distance_km < max_distance_km` ve `min_students <= student_count <= max_students`.

**Mesafe eşikleri gidiş-dönüş km cinsindendir** (§4). Tüm sütunlar arayüzden düzenlenebilir: km eşikleri, öğrenci sayısı eşikleri ve saatler değiştirilebilir, satır eklenip silinebilir.

Bir işletme birden fazla kurala uyarsa **en dar aralıklı** kural seçilir. "En dar" sıralaması kesin olarak şudur:

1. En küçük mesafe aralığı genişliği (`max_distance_km - min_distance_km`; `NULL` üst sınır sonsuz sayılır)
2. Eşitlikte en küçük öğrenci aralığı genişliği (`max_students - min_students`; `NULL` üst sınır sonsuz sayılır)
3. Hâlâ eşitse en küçük `id`

Hiçbir kurala uymazsa `RuleNotFound` ihlali üretilir ve saat `0` kalır — sessiz varsayılan atanmaz.

Tam CRUD'lu; Ayarlar ekranından düzenlenir.

### `assignments` — atamalar

| Sütun | Tip | Not |
|---|---|---|
| `id` | INTEGER PK | |
| `teacher_id` | INTEGER FK → `teachers` | |
| `company_id` | INTEGER FK → `companies` | |
| `awarded_hours` | INTEGER NOT NULL | **takdir edilen** haftalık saat |
| `max_hours_snapshot` | INTEGER NOT NULL | atama anındaki tavan |
| `is_forced` | INTEGER | `0` / `1` — zorlama ekleme |
| `force_reason` | TEXT NULL | `is_forced = 1` ise zorunlu |
| `term` | TEXT NOT NULL | |
| `created_at`, `updated_at` | TEXT | ISO-8601 |

**UNIQUE(`company_id`, `term`)** — bir işletme dönem başına tek koordinatöre bağlanır.

`max_hours_snapshot` türetilmiş bir değerdir ve normalde saklanmazdı. Burada saklanmasının sebebi hesap değil **belge bütünlüğüdür**: imzalanmış bir çizelge, kural tablosu sonradan değiştirildiğinde kendiliğinden değişmemelidir.

### `assignment_slots` — atama zaman dilimleri

| Sütun | Tip | Not |
|---|---|---|
| `id` | INTEGER PK | |
| `assignment_id` | INTEGER FK → `assignments` ON DELETE CASCADE | |
| `day_of_week` | INTEGER | 1–5 |
| `hour` | INTEGER | `day_start_hour` … `day_end_hour` |

**UNIQUE(`assignment_id`, `day_of_week`, `hour`)**

Her satır bir saatlik ziyaret dilimidir. `COUNT(slots) == awarded_hours` olmalıdır; olmazsa `UnplacedHours` ihlali üretilir. Bu yapı, takdir edilen 4 saatin gerekirse iki güne bölünmesine izin verir.

**Saatler neden tam sayı:** İşletmede yapılan ders saati 60 dakikadır (§3.3) ve bir dilim bir saattir. `awarded_hours` kesirli olsaydı `COUNT(slots)` ona hiçbir zaman eşit olamaz, `UnplacedHours` sürekli tetiklenirdi. Bu nedenle `max_hours`, `max_hours_snapshot` ve `awarded_hours` tam sayıdır.

### `settings` — ayarlar

Anahtar/değer tablosu:

| Anahtar | Örnek |
|---|---|
| `school_name` | `Atatürk Mesleki ve Teknik Anadolu Lisesi` |
| `school_latitude`, `school_longitude` | okul konumu, mesafe hesabı için |
| `institution_type` | `other` / `vocational_center` |
| `is_metropolitan_district` | `true` |
| `active_term` | `2026-2027/1` |
| `day_start_hour`, `day_end_hour` | `8`, `17` — müsaitlik ızgarasının sınırları |
| `branch_weekly_hours` | JSON: sınıf+dal → haftalık uygulamalı meslek dersi saati |
| `branch_group_counts` | JSON: sınıf+dal → grup sayısı |

---

## 7. Hesap motoru — `domain/workload.rs`

Saf fonksiyonlar. Veritabanı ve Tauri bağımlılığı yoktur; girdi olarak yalın veri tipleri alır. En çok test edilmesi gereken katman burasıdır.

### 7.1 Öğretmen koordinatörlük kapasitesi

```
chief_hours = match chief_type {
    department   => 10,   // MADDE 6/4
    workshop_lab => 6,    // MADDE 6/4
    none         => 0,
}

statutory_cap = match (institution_type, is_metropolitan_district) {
    (vocational_center, true)  => 24,   // MADDE 15/2-a-1
    (vocational_center, false) => 18,   // MADDE 15/2-a-2
    (other,             true)  => 20,   // MADDE 15/2-b-1
    (other,             false) => 16,   // MADDE 15/2-b-2
}

remaining_budget = max_extra_hours - chief_hours - other_extra_hours
capacity         = min(statutory_cap, remaining_budget)
```

Bu okul için (`other`, büyükşehir ilçesi, `max_extra_hours = 24`, `other_extra_hours = 0`):

| Şeflik | Hesap | Kapasite |
|---|---|---|
| Bölüm şefi | `min(20, 24 − 10)` | **14 saat** |
| Atölye/lab şefi | `min(20, 24 − 6)` | **18 saat** |
| Şef değil | `min(20, 24 − 0)` | **20 saat** |

### 7.2 İşletme saat tavanı

```
round_trip_distance_km = company.one_way_distance_km × 2

max_hours_for(company) =
    company_hour_rules içinde round_trip_distance_km ve company.student_count
    değerlerine uyan en dar kuralın max_hours değeri
```

### 7.3 Takdir edilen saat

```
awarded_hours = kullanıcı girdisi
kısıt: 0 < awarded_hours <= max_hours     (aşımda AwardedExceedsMax ihlali)
```

Tavan hesaplanır, takdir girilir. Örnek: hesap 6 saat verir, kullanıcı 4 saat takdir eder.

### 7.4 Okul toplam havuzu

```
total_pool_hours = Σ over (grade, branch) of
                   branch_weekly_hours[grade][branch] × branch_group_counts[grade][branch]
```

MADDE 15/2 uyarınca hesaplanır; tüm takdirlerin toplamı bu değeri aşarsa `PoolExceeded` ihlali üretilir. Bu bir **arz** büyüklüğüdür; §7.1'deki kapasite ise **kişi başı kısıttır**. İkisi karıştırılmamalıdır.

---

## 8. Zamanlama modeli — `domain/scheduling.rs`

Bir atamanın zaman dilimleri şu kümelerin kesişiminde olmalıdır:

1. **Öğretmenin boş saatleri** — `teacher_availability` (gün, saat)
2. **Öğrencilerin işletmede bulunduğu günler** — o işletmedeki tüm öğrencilerin sınıflarının `class_workplace_days` birleşimi
3. **Günlük 8 saat sınırı** — OÖKY MADDE 88

```
eligible_slots(assignment) =
      teacher_free_slots(teacher_id, term)
    ∩ { (day, hour) : day ∈ workplace_days(company.students) }
```

Bir işletmede farklı sınıflardan öğrenci varsa (ör. 12/C ve 12/D), kullanılabilir günler bu sınıfların günlerinin **birleşimidir**; ancak seçilen gün yalnızca o gün işletmede olan öğrencileri kapsar. Bu durum `PartialStudentCoverage` bilgi notu üretir — ihlal değildir, çünkü koordinatör tek ziyarette tüm öğrencileri göremeyebilir.

### 8.1 Zorlama ekleme

Uygun dilim bulunamadığında veya kullanıcı bilinçli olarak kural dışına çıkmak istediğinde, atama satırında **"Zorla ekle"** anahtarı açılır:

- `assignments.is_forced = 1` olur ve `force_reason` **zorunlu** metin alanı haline gelir
- İlgili ihlaller engellemez; `warning` seviyesine düşer ve kayıtta saklanır
- Görevlendirme çizelgesi PDF'inde zorlanmış satırlar işaretlenir ve belge başına uyarı bandı basılır

Engellemek yerine göstermek bilinçli bir tasarım kararıdır: müdür bazen ulaşım veya öğretmen yokluğu nedeniyle mevzuat sınırında karar verir. Motor bunu yasaklarsa kullanıcı uygulamayı bırakıp elektronik tabloya döner.

---

## 9. Dağıtım öneri motoru — `domain/allocation.rs`

Saf fonksiyon. Girdi: işletmeler, öğretmenler, müsaitlikler, sınıf günleri, kural tablosu, ayarlar. Çıktı: `Vec<ProposedAssignment>` + `Vec<Violation>`.

1. **Uygunluk filtresi** — işletmenin dalı öğretmenin `branches` listesinde mi? Değilse "yakın alan" ikinci öncelik olarak değerlendirilir (OÖKY MADDE 88).
2. **Sıralama** — işletmeler öğrenci sayısına göre azalan, eşitlikte okula uzaklığa göre azalan.
3. **Yerleştirme** — uygun öğretmenler arasından, kalan kapasitesi `max_hours`'u karşılayan ve mevcut işletmelerinin coğrafi merkezine **en yakın** olanı seç. Bu, MADDE 88'in *"işletmeler arası uzaklık"* kıstasının karşılığıdır ve bir öğretmenin işletmelerinin şehre dağılmasını engeller.

   Buradaki yakınlık yalnızca bir **sıralama sinyalidir**, resmî mesafe değildir: koordinatlar arası düz çizgi farkı, hangi öğretmenin adayı olduğunu seçmek için kullanılır. Saat tavanına giren tek mesafe `one_way_distance_km × 2`'dir (§7.2) ve o değer bu adımdan etkilenmez.
4. **Dilim yerleştirme** — `eligible_slots` içinden `awarded_hours` kadar dilim seç; aynı güne bitişik saatleri tercih et (ziyaret verimliliği), günlük 8 saati aşma.
5. **Yerel iyileştirme** — iki öğretmen arasında işletme takası toplam yolu kısaltıyor ve her iki tarafın kapasitesini/dilimlerini bozmuyorsa uygula. Sabit tur sayısında durur.

Öneri, başlangıçta `awarded_hours = max_hours` koyar; kullanıcı aşağı çeker.

28 işletme ve yaklaşık 10 öğretmen ölçeğinde çalışma süresi saniyenin altındadır; bu nedenle tam optimizasyon yerine açgözlü yerleştirme + yerel iyileştirme tercih edilmiştir (YAGNI).

---

## 10. Denetlenen ihlaller — `domain/validation.rs`

| Kod | Seviye | Kural | Kaynak |
|---|---|---|---|
| `AwardedExceedsMax` | error | `awarded_hours > max_hours_snapshot` | `company_hour_rules` |
| `WeeklyCapExceeded` | error | Öğretmenin toplam `awarded_hours`'u `capacity`'yi aşıyor | MADDE 15/2 + MADDE 6/1-c |
| `DailyCapExceeded` | error | Aynı `day_of_week` toplamı 8 saati aşıyor | OÖKY MADDE 88 |
| `SlotNotAvailable` | error | Dilim öğretmenin boş saatlerinde değil | `teacher_availability` |
| `SlotNotWorkplaceDay` | error | Dilimin günü öğrencilerin işletme günlerinde değil | `class_workplace_days` |
| `UnplacedHours` | error | `COUNT(slots) ≠ awarded_hours` | — |
| `TooManyStudentsPerCoordinator` | error | Aynı işletmede aynı alanda 15'ten fazla öğrenci | OÖKY MADDE 88 |
| `PoolExceeded` | warning | Takdir toplamı `total_pool_hours`'u aşıyor | MADDE 15/2 |
| `FieldMismatch` | warning | İşletmenin dalı öğretmenin dallarında yok | OÖKY MADDE 88 |
| `RuleNotFound` | warning | İşletme hiçbir km/öğrenci aralığına düşmüyor | — |
| `MissingCoordinates` | warning | `latitude` / `longitude` yok, mesafe kıstası uygulanamadı | — |
| `PartialStudentCoverage` | info | Seçilen gün işletmedeki öğrencilerin bir kısmını kapsıyor | — |

`is_forced = 1` olan atamalarda `error` seviyesindeki ihlaller `warning`'e düşer ve kayıtta saklanır.

---

## 11. Coğrafi kodlama — `services/geocoding.rs`

- Sağlayıcı: **OpenStreetMap Nominatim** (API anahtarı gerektirmez)
- Sorgulanan metin: CSV'nin `Address:` satırı, `Result:` satırı değil
- **Hız sınırı: saniyede en fazla 1 istek.** OSM kullanım koşulları gereği zorunludur; ayrıca tanımlayıcı bir `User-Agent` başlığı gönderilir. Uyulmazsa IP engellenir. 28 işletme yaklaşık 30 saniye sürer.
- Başarısız sonuçlar `geocode_status = failed` işaretlenir; kullanıcı İşletmeler ekranında haritadan işaretleyerek düzeltir, durum `manual` olur.
- **Coğrafi kodlama mesafeyi hesaplamaz.** `one_way_distance_km` yalnızca CSV'den gelir veya kullanıcı tarafından elle girilir/düzeltilir. Koordinat bulunması bu değeri **değiştirmez**.

  Gerekçe: CSV'deki `Distance:` değeri formun ürettiği **araç yol mesafesidir**. Koordinatlardan hesaplanabilecek tek şey kuş uçuşu (haversine) mesafedir ve bu, şehir içinde gerçek yol mesafesinden belirgin biçimde kısadır. Gerçek veriyi tahminle ezmek saat tavanını yanlış aralığa düşürür. Koordinatlar yalnızca **harita üzerinde gösterim** ve öneri motorunun kümeleme sinyali için kullanılır (§9).

---

## 12. Arayüz

### 12.1 Navigasyon — kenar çubuğu ve `Drawer`

Uygulama kabuğu `SidebarLayout` > (`Sidebar` + `SidebarMain`) şeklinde kurulur.

Geniş ekranda kalıcı bir `<aside>` kenar çubuğu, dar ekranda (`< 900px`) OpenVue `Drawer`
bileşeni kullanılır. Üst çubuktaki `pi pi-bars` düğmesi geniş ekranda çubuğu ikon moduna
daraltır, dar ekranda Drawer'ı açar. Menü öğeleri `RouterLink` olup aktif rota
`.router-link-active` sınıfıyla vurgulanır. Renkler OpenVue tasarım belirteçlerinden
(`--p-content-border-color`, `--p-highlight-background` …) okunur, sabit renk yazılmaz.

Menü grupları:

- **Kayıtlar** — İşletmeler · Öğrenciler · Öğretmenler
- **Planlama** — Dağıtım · Müsaitlik Takvimi
- **Raporlar** — Görevlendirme Çizelgesi · Öğretmen Ziyaret Listeleri
- **Yönetim** — Ayarlar · İçe/Dışa Aktarım

### 12.2 Ekranlar

| Ekran | İçerik |
|---|---|
| **Genel Bakış** | Sayaçlar (işletme / öğrenci / öğretmen / atanmamış işletme) · ihlal özeti rozetleri · havuz kullanımı çubuğu (`Σ awarded_hours / total_pool_hours`) |
| **İşletmeler** | `Splitter`: solda `DataTable` (filtre, sıralama, sayfalama), sağda Leaflet haritası. Satır seçimi marker'ı vurgular, marker tıklaması satırı seçer. `geocode_status` renkli `Tag`. Tam CRUD, toplu coğrafi kodlama butonu, haritadan elle konum düzeltme. |
| **Öğrenciler** | `DataTable` tam CRUD; işletme ve dal `Select` ile bağlanır; sınıf ve dal filtresi. |
| **Öğretmenler** | `DataTable` tam CRUD; kapasite çubuğu (`awarded / capacity`); `chief_type` `Select`, türetilen şeflik saati salt-okunur gösterilir. |
| **Müsaitlik Takvimi** | Öğretmen seçilir, gün × saat ızgarasında boş saatler işaretlenir (tıkla veya sürükle). Aynı ekranda sınıfların işletme günleri (`class_workplace_days`) düzenlenir. Kendi `AvailabilityGrid.vue` bileşenimiz kullanılır; OpenVue'da hazır takvim/zamanlama bileşeni yoktur. |
| **Dağıtım** | Ana ekran. Solda öğretmen kartları (kapasite çubuğu, atanmış işletmeler, haftalık dilim ızgarası), sağda harita (marker rengi = atanan öğretmen, gri = atanmamış). "Öneri Üret" butonu motoru çalıştırır. Kart ↔ kart sürükle-bırak ile işletme taşınır. Her atama satırında `Tavan 6 sa. · Takdir [4]` `InputNumber` ve dilim yerleştirme ızgarası. İhlaller anında `Tag` olarak görünür; "Zorla ekle" anahtarı ve gerekçe alanı satır içindedir. |
| **Ayarlar** | Okul bilgisi ve konumu · `institution_type` · `is_metropolitan_district` · aktif dönem · gün başlangıç/bitiş saati · sınıf+dal bazlı haftalık ders saati ve grup sayısı · **Saat Tavanı Kuralları** ızgarası (tam CRUD) |
| **İçe/Dışa Aktarım** | CSV içe aktarma sihirbazı · Excel/CSV dışa aktarma · rapor bağlantıları |

### 12.3 CSV içe aktarma sihirbazı

1. **Dosya seç ve önizle** — ilk 10 satır `DataTable`'da, sütun eşlemesi gösterilir
2. **Mükerrer kontrolü** — işletme adı Unicode-doğru normalize edilerek (`to_lowercase()` + boşluk sadeleştirme) mevcut kayıtlarla karşılaştırılır; her mükerrer için *atla / güncelle / yeni kayıt* seçimi sunulur. 32 öğrenci → 28 tekil işletme birleştirmesi bu adımda olur.
3. **İçe aktar ve coğrafi kodla** — kayıtlar yazılır, Nominatim kuyruğu 1 istek/saniye ile çalışır, `ProgressBar` ile ilerleme gösterilir.

### 12.4 Konum seçme — iki kullanım, tek bileşen

Harita üzerinden konum işaretleme **iki yerde** gerekir ve ikisi de aynı
`LocationPickerMap.vue` bileşenini kullanır:

1. **İşletme konumu** — İşletmeler ekranında; coğrafi kodlama tutmadığında veya
   sonucu düzeltmek gerektiğinde. Yazılan yer: `companies.latitude` / `companies.longitude`,
   durum `geocode_status = 'manual'`.
2. **Okul konumu** — Ayarlar ekranında. Yazılan yer: `settings.school_latitude` /
   `settings.school_longitude`.

Bileşen sözleşmesi: `modelValue` olarak `{ latitude, longitude } | null` alır, haritaya
tıklandığında veya işaret sürüklendiğinde güncellenmiş değeri yayar. Konum boşken harita
okul konumuna, o da boşsa Mersin merkezine odaklanır. Enlem/boylam ayrıca sayı girdisi
olarak da düzenlenebilir; harita ve girdiler tek yönlü değil, çift yönlü bağlıdır.

**Okul konumu mesafe hesabında kullanılmaz** (§11). Kullanım amacı harita odağı ve
dağıtım motorunun kümeleme referansıdır (§9).

### 12.5 Leaflet entegrasyon uyarısı

Leaflet kendi DOM'unu yönetir. Marker nesneleri `ref()` içine konursa Vue onları proxy'ler ve Leaflet'in iç referans karşılaştırmaları bozulur. Marker'lar `shallowRef` içinde veya bileşen dışı bir `Map<id, Marker>` yapısında tutulur.

---

## 13. Çıktılar

1. **Koordinatör görevlendirme çizelgesi (PDF)** — MADDE 15/2'nin *"okul ve kurum müdürlüğünce hazırlanacak ve millî eğitim müdürlüğünce onaylanacak program"* belgesi. Sütunlar: öğretmen, işletme, adres, öğrenci sayısı, öğrenciler, ziyaret gün/saatleri, haftalık saat. Öğretmen bazında ara toplam, genel toplam, altta imza blokları (Koordinatör Müdür Yardımcısı · Okul Müdürü · İl/İlçe MEM Onayı). Zorlanmış satırlar işaretlenir; belge başında uyarı bandı basılır.
2. **Öğretmen başına haftalık ziyaret listesi** — tek sayfa: işletme adı, adres, telefon, yetkili, öğrenciler, gün/saat, takdir edilen saat. Yazdırılabilir; telefonda da açılabilir.
3. **Excel/CSV dışa aktarım** — `rust_xlsxwriter` ile atama tablosu, işletme listesi, öğrenci listesi ayrı sayfalar halinde.

PDF üretimi webview yazdırma ile yapılır: rapor ayrı bir rota olarak HTML render edilir, `@media print` ile A4 ve sayfa kırılımları ayarlanır, pencereden yazdır → PDF olarak kaydet.

---

## 14. Dosya yapısı

```
MESNET.Lite/
├── src-tauri/
│   ├── migrations/
│   │   ├── 0001_initial.sql
│   │   └── 0002_seed_hour_rules.sql
│   ├── src/
│   │   ├── main.rs
│   │   ├── error.rs                        -- AppError, tek hata tipi
│   │   ├── db/
│   │   │   ├── mod.rs                      -- havuz, migration çalıştırma
│   │   │   ├── companies.rs
│   │   │   ├── students.rs
│   │   │   ├── teachers.rs
│   │   │   ├── availability.rs
│   │   │   ├── class_days.rs
│   │   │   ├── assignments.rs
│   │   │   ├── hour_rules.rs
│   │   │   └── settings.rs
│   │   ├── domain/
│   │   │   ├── models.rs                   -- serde tipleri
│   │   │   ├── workload.rs                 -- kapasite ve tavan hesabı
│   │   │   ├── scheduling.rs               -- uygun dilim hesabı
│   │   │   ├── allocation.rs               -- dağıtım öneri motoru
│   │   │   └── validation.rs               -- ihlal denetimi
│   │   ├── services/
│   │   │   ├── geocoding.rs
│   │   │   ├── csv_import.rs
│   │   │   └── excel_export.rs
│   │   └── commands/
│   │       ├── mod.rs
│   │       ├── company_commands.rs
│   │       ├── student_commands.rs
│   │       ├── teacher_commands.rs
│   │       ├── availability_commands.rs
│   │       ├── assignment_commands.rs
│   │       ├── import_export_commands.rs
│   │       └── settings_commands.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/
│   ├── main.ts
│   ├── App.vue
│   ├── router/index.ts
│   ├── stores/
│   ├── api/                                -- tipli invoke sarmalayıcıları
│   ├── i18n/labels.ts                      -- Türkçe etiket sözlüğü
│   ├── components/
│   │   ├── layout/AppSidebar.vue
│   │   ├── map/LocationPickerMap.vue
│   │   ├── map/CompanyMap.vue
│   │   ├── schedule/AvailabilityGrid.vue
│   │   ├── allocation/TeacherCard.vue
│   │   ├── allocation/AssignmentRow.vue
│   │   └── common/ViolationBadge.vue
│   └── views/
│       ├── DashboardView.vue
│       ├── CompaniesView.vue
│       ├── StudentsView.vue
│       ├── TeachersView.vue
│       ├── AvailabilityView.vue
│       ├── AllocationView.vue
│       ├── SettingsView.vue
│       ├── ImportExportView.vue
│       └── reports/
│           ├── AssignmentSheetReport.vue
│           └── TeacherVisitListReport.vue
├── docs/superpowers/specs/
└── CLAUDE.md
```

Hedef: Rust dosyaları 200–400 satır, 800 satır üst sınır. `commands/` katmanı iş mantığı içermez; yalnızca `db/` ve `domain/` çağırır.

---

## 15. Hata yönetimi

- Rust tarafında tek bir `AppError` enum'u; `thiserror` ile tanımlanır, `serde::Serialize` uygular ve Tauri komutlarından `Result<T, AppError>` olarak döner.
- Sınır doğrulaması: CSV içe aktarımı, Nominatim yanıtı ve tüm komut girdileri kullanıma alınmadan önce doğrulanır. Dış veri güvenilmez kabul edilir.
- Frontend `Toast` ile Türkçe, kullanıcıya anlamlı mesaj gösterir; teknik ayrıntı konsola yazılır.
- Hiçbir hata sessizce yutulmaz. Coğrafi kodlama başarısızlığı bir hata değil, kaydedilen bir **durumdur** (`geocode_status = failed`) ve arayüzde görünür.

---

## 16. Test yaklaşımı

TDD uygulanır: önce mevzuattan türeyen test yazılır, sonra implementasyon.

**`domain/` katmanı saf fonksiyonlardan oluşur; veritabanı ve Tauri gerektirmez. Hedef kapsam %100'e yakın.**

Örnek test vakaları:

- `workload`: bölüm şefi kapasitesi 14 · atölye/lab şefi 18 · şef olmayan 20 · meslekî eğitim merkezi + büyükşehir 24 · büyükşehir dışı diğer okul 16
- `workload`: `max_hours` en dar kuralı seçer; hiçbir kurala uymayan işletme `RuleNotFound` üretir
- `workload`: `awarded_hours > max_hours` → `AwardedExceedsMax`
- `scheduling`: öğretmenin boş olmadığı dilim → `SlotNotAvailable`
- `scheduling`: sınıfın işletme günü olmayan gün → `SlotNotWorkplaceDay`
- `scheduling`: aynı gün 9 saatlik dilim → `DailyCapExceeded`
- `scheduling`: `is_forced = 1` → aynı ihlaller `warning` seviyesine düşer
- `validation`: `COUNT(slots) ≠ awarded_hours` → `UnplacedHours`
- `csv_import`: üç satırlı `İşletmenizi bulun` alanı doğru ayrıştırılır; `Result:` yok sayılır
- `csv_import`: aynı işletme adının farklı büyük/küçük harfli yazımları Unicode-doğru şekilde tek kayıt sayılır

Entegrasyon testleri `db/` katmanı için geçici SQLite dosyası üzerinde çalışır. Proje geneli hedef kapsam **%80+**.

---

## 17. Kurulum verisi — `company_hour_rules` seed

`migrations/0002_seed_hour_rules.sql` içine yazılacak başlangıç tablosu. **Satır eksenindeki km değerleri gidiş-dönüş mesafesidir** (tek yönün iki katı, §4).

| Gidiş-dönüş km | 1–2 öğrenci | 3–4 öğrenci | 5–6 öğrenci | 6+ öğrenci |
|---|---|---|---|---|
| 0 – 1 km | 2 | 3 | 4 | 5 |
| 1 – 3 km | 4 | 5 | 6 | 7 |
| 3 – 5 km | 6 | 7 | 8 | 9 |
| 5 km + | 8 | 9 | 10 | 11 |

16 satır olarak seed edilir. Son km satırında `max_distance_km = NULL`, son öğrenci sütununda `max_students = NULL`.

**`5-6` ve `6+` sütunları 6 öğrencide çakışır.** §6'daki "en dar kural" sıralaması bunu belirlenimci şekilde çözer: `5-6` aralığı daha dar olduğu için 6 öğrencili işletme `5-6` sütununu alır. Kullanıcı bunu `7+` yapmak isterse tek satırlık `UPDATE` yeterlidir.

Tüm değerler (km eşikleri, öğrenci eşikleri, saatler) Ayarlar ekranından değiştirilebilir; seed yalnızca başlangıç durumudur.

### 17.1 Mevcut veri üzerindeki etkisi

CSV'deki 28 işletme için gidiş-dönüş dönüşümü sonrası dağılım:

| Aralık | Tek yön | Gidiş-dönüş |
|---|---|---|
| 0 – 1 km | 3 | **0** |
| 1 – 3 km | 7 | 5 |
| 3 – 5 km | 4 | 4 |
| 5 km + | 14 | **19** |

Öğrenci sayısı: 24 işletme 1 öğrencili, 4 işletme 2 öğrencili — **hepsi `1–2 öğrenci` sütununda**. Bu veri setinde öğrenci ekseni devreye girmiyor.

Toplam talep: `19 × 8 + 4 × 6 + 5 × 4 = 196 saat`. Öğretmen başına tavan 20 saat olduğundan bu dağıtım **en az 10 koordinatör öğretmen** gerektirir; şeflik görevi olanlarda kapasite 14–18 saate düştüğü için pratikte daha fazlası gerekir. Öğretmen sayısı yetersizse ilk dağıtımda `PoolExceeded` ve `WeeklyCapExceeded` ihlalleri toplu halde çıkar — bu beklenen davranıştır, hata değildir.

---

## 18. Kapsam dışı (YAGNI)

- Ek ders **ücreti** (TL) hesabı — kullanıcı açıkça kapsam dışı bıraktı
- Çok kullanıcılı erişim, sunucu, kimlik doğrulama — tek kullanıcılı masaüstü uygulaması
- MEBBİS / e-Okul entegrasyonu — veri CSV ile girer, Excel ile çıkar
- Devamsızlık, beceri sınavı, sözleşme takibi
- Rota/mesafe hesaplama servisi (OSRM, Google Directions vb.) — yol mesafesi CSV'den gelir, gerekirse elle düzeltilir
- Çevrimdışı harita tile önbelleği
