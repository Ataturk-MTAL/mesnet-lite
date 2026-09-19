# Dönem İçi Değişiklikler ve Geriye Dönük Tarihçe — Tasarım (Faz A)

- **Tarih:** 2026-09-19
- **Durum:** Kullanıcı onayladı
- **Dal:** `feat/change-history`

## 1. Sorun

Dönem içinde şu değişiklikler olur:

- Öğrenci işletmeden ayrılır ya da başka bir işletmeye geçer. Hedef işletme veritabanında yoksa o anda oluşturulur.
- Öğretmenin okuldaki ders saati, haftalık programı, şefliği ya da kadro durumu değişir.
- İşletmenin koordinatörü başka bir öğretmene geçer.

Bu değişiklikler işletme saat tavanını, takdir edilen saati ve koordinatörlük ek dersini zincir hâlinde etkiler. **Hepsi geriye dönük izlenebilmelidir.**

Bugünkü şemada tarihçe yok:

- `students.company_id` yerinde üzerine yazılıyor.
- `company_term_hours` her dönem için tek satır tutuyor.
- `teachers` tablosunun yük sütunları tarihsiz.
- `terms` diye bir tablo yok, dönemlerin tarihi yok.

MESNET event sourcing kullanıyordu (Marten; `TeacherSchedule` akışında `ScheduleCreated` ve `ScheduleUpdated`). Ancak yalnız **kayıt zamanını** tutuyordu. "10 Kasım'da girilen, 3 Kasım'dan geçerli değişiklik" sorusunu cevaplayamıyordu.

## 2. Kullanıcı kararları (bağlayıcı)

1. **Tarihçenin iki kullanımı var.**
   - Denetim listesi: kim, ne zaman, neden, önce → sonra.
   - Tarih itibarıyla durum: "X gününde durum neydi".
2. **Yürürlük tarihi, kayıt zamanından ayrıdır.**
   - Yürürlük tarihi = yeni durumun geçerli olduğu **ilk gün**. "3 Kasım'da ayrıldı" girilirse 3 Kasım eski işletmede sayılmaz.
   - Yürürlük tarihi belgeden okunur. Formdaki alan adları: "Sözleşme başlangıç tarihi", "Sözleşme fesih tarihi", "Geçerlilik tarihi".
3. **Ay penceresi.** Ek ders puantajı ay sonunda mutemede verilir ve en geç ayın 3'ünde ilçeye gönderilir. Bu yüzden:
   - Yürürlük tarihi, **içinde bulunulan ayın 1'inden önce olamaz.** Ay içindeki her işlem ay sonuna kadar girilebilir ve yürürlük tarihinden itibaren geçerli olur.
     - Örnek: 25 Ekim'de girilen, 10 Ekim tarihli sözleşme kabul edilir. Ekim puantajı 10 Ekim'den ay sonuna kadar buna göre çıkar.
   - Önceki ayın puantajı **değişemez**. Önceki aya düşen tarih reddedilir.
     - Kullanıcı "ayın 1'inden gir" seçeneğini seçerse gerçek belge tarihi ayrı bir `document_date` alanında saklanır.
   - Ayın 1–3'ü arasında önceki ay için tolerans **yoktur**.
   - Gelecek tarih dönem sonuna kadar serbesttir.
   - Geri alma ve düzeltme de aynı pencereye tabidir.
4. **Zincir etki: otomatik uygula ve bildir.**
   - Öğrenci ayrılınca işletmenin tavanı düşerse, tavanı aşan saat aynı tarihte tavana indirilir. **Kilitli satırlar indirilmez**, yalnız uyarı verir.
   - Öğrencisi kalmayan işletmenin koordinatör ataması biter, saati 0'a iner.
   - Tavan yükselirse yalnız bildirim verilir; saat kendiliğinden artmaz.
   - Yerinde oluşturulan işletmenin saati ve ataması elle girilir.
   - Tüm etkiler tek bir etki penceresinde özetlenir.
5. **Öğretmen tarafında tarihçesi tutulanlar:**
   - okuldaki ders saati (`other_extra_hours`),
   - haftalık program ve boş saatler,
   - şeflik ve kadro durumu,
   - koordinatör değişimi.

   Kapasite aşılırsa yalnız bayrak konur. Otomatik düşürme yapılmaz, çünkü hangi işletmenin düşeceği belirsizdir.
6. **Sayım birimi gündür.** Her ziyaret günü, o tarihte geçerli durumla sayılır.
7. **Göç.** Mevcut veri, **dönem başından** geçerli açılış olayları olarak yazılır. Dönem tarihleri varsayılanla gelir ve "onaylanmadı" olarak işaretlenir:
   - `/1` için 1 Eylül – 31 Ocak,
   - `/2` için 1 Şubat – 30 Haziran.
8. **Kapsam iki fazdır.**
   - Faz A: bu belge.
   - Faz B: aylık ek ders puantajı.

## 3. Mimari karar

**Olay günlüğü doğruluk kaynağıdır.** Tarih aralıklı tablolar bu günlükten yeniden kurulan projeksiyonlardır. Bir event sourcing kütüphanesi bağımlılık olarak alınmaz, ama esrs'in desenleri kullanılır.

Kütüphaneler neden elendi (kaynak kodu okunarak doğrulandı, 2026-09-19):

- **esrs (primait/event_sourcing.rs):**
  - Son sürüm `0.18.0` (2024-11-25). `0.19.0` yayınlanmadı.
  - SQLite store'u `0.7.0`'da kaldırıldı. Tek store `PgStore`: advisory lock, `jsonb`, kendi çalıştırdığı DDL.
  - Tek zaman alanı `occurred_on`, `persist` içinde `Utc::now()` ile yazılıyor; yürürlük tarihi yok.
  - Her aggregate için ayrı transaction açıyor, bu yüzden zincir etki atomik olamıyor.
  - Aggregate kimliği `Uuid`, bizde `INTEGER`.
  - Repoda LICENSE dosyası yok.
- **eventsourcing:** Son commit 2020-03-11. SQL backend yok, `apply_all` hata durumunda panic ediyor.
- **cqrs-es + sqlite-es:** Projeksiyon commit'ten sonra çalışıyor ve hatayı yutuyor.
- Hiçbiri yürürlük tarihini modellemiyor.

esrs'ten alınan desenler (kodda esrs'e atıf yapılır):

- Saf `decide(ctx, cmd) -> Result<Decision, Rejection>` ve saf `apply(state, event) -> state`.
- İki katmanlı sonuç: `AppResult<ChangeOutcome>`. `Rejected` alan kuralı, `Err` ise DB/IO hatasıdır.
- Projeksiyon, olayla aynı transaction'da yazılır (`TransactionalEventHandler` karşılığı).
- Upcast için `kind_version` ile tek bir `match (kind, version)`.
- Rebuild, test kâhini olarak kullanılır: yeniden kurulan projeksiyon canlı projeksiyona eşit olmalı.

## 4. Veri modeli

`0006_history.sql` yalnız ekleme yapar. Mevcut testler her ara adımda yeşil kalır. `0007_history_cleanup.sql` ise eski depolamayı, tüm okuyucu ve yazıcılar yeni tablolara geçtikten **sonra** kaldırır.

### 4.1 Tablolar (`0006`)

- **`terms`**: `term PK`, `start_date`, `end_date`, `dates_confirmed`, `created_at`. `STRICT`.
- **`change_sets`**: bir kullanıcı eylemini temsil eder.
  - Sütunlar: `id AUTOINCREMENT`, `term`, `kind`, `effective_date`, `document_date` (NULL olabilir), `reason`, `actor`, `recorded_at` (UTC RFC3339, milisaniye), `revokes_change_set_id UNIQUE`, `impact_json`.
- **`change_events`**: doğruluk kaynağı.
  - Sütunlar: `id AUTOINCREMENT` (küresel kayıt sırası), `change_set_id`, `stream`, `subject_id` (FK yok; kayıt satırdan uzun yaşar), `term`, `kind`, `kind_version`, `effective_date`, `payload` (JSON), `caused_by`, `revokes UNIQUE`.
  - Kısıt: `CHECK ((kind = 'revoked') = (revokes IS NOT NULL))`.
- **Değişmezlik:** iki günlük tablosunda da `BEFORE UPDATE` ve `BEFORE DELETE` tetikleyicileri `RAISE(ABORT, …)` verir.
- **Beş projeksiyon.** Tek yazıcısı `db/projection.rs`'tir. Aralıklar yarı açıktır: `[valid_from, valid_to)`.
  - `student_placements` (`student_id`, `company_id`)
  - `company_hour_periods` (`awarded_hours`, `max_hours_snapshot`, `is_honorary`, `is_locked`, `notes`)
  - `coordination_periods` (`teacher_id`, `visit_day`, `visit_hour`, `is_forced`, `force_reason`)
  - `teacher_load_periods` (`base_hours`, `max_extra_hours`, `other_extra_hours`, `chief_type`, `employment_type`)
  - `teacher_schedule_periods` (`slots_json` = `[[gün, saat], …]` biçiminde BOŞ saatler)

  Hepsinde ortak olanlar:
  - `source_event_id` sütunu;
  - açık satır için kısmi unique index (`WHERE valid_to IS NULL`);
  - `UNIQUE (özne, term, valid_from)`;
  - `CHECK (valid_to IS NULL OR valid_to > valid_from)`;
  - **çakışma yedeği:** tarih aralığı örtüşen bir satır eklenirse `RAISE(ABORT)` veren bir `BEFORE INSERT` tetikleyicisi.
- **`companies.is_active`**: `INTEGER NOT NULL DEFAULT 1`. Yumuşak silme için kullanılır.
- **`settings`**: `operator_name` anahtarı eklenir (varsayılan `''`); `actor` değeri buradan gelir.

**Tarih doğrulaması:** Her tarih sütunu `CHECK (date(x) IS x)` taşır. `= x` kullanılmaz, çünkü `'garbage'` gibi bir değeri kabul eder.

**Uyumluluk:** Bundled SQLite 3.46.0 ile uyumlu olmalıdır.

### 4.2 Akışlar ve olaylar

Her olayın yükü şunları taşır:

- **sonuç durumu** (sıralamaya dayanıklı, MESNET `ScheduleUpdated` gibi);
- **beklenen önceki değer** (yerleştirme ve koordinasyon olaylarında `from_*`);
- **`labels`** (kayıt anındaki ad anlık görüntüsü; yeniden adlandırma geçmişi bozmaz).

| Akış (`subject`) | Olaylar |
|---|---|
| `placement` (öğrenci) | `student_placed{to_company_id, from_company_id?, source}`, `student_transferred{from_company_id, to_company_id, to_company_created}`, `student_left{from_company_id}` |
| `company_hours` (işletme) | `hours_set{awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes, previous_awarded?}`, `hours_capped{cap, student_count}` **(sınırlama)**, `hours_cleared` (yalnız planlamada) |
| `coordination` (işletme) | `coordinator_assigned{teacher_id, visit_day, visit_hour, is_forced, force_reason?, from_teacher_id?}`, `coordinator_ended{from_teacher_id}`, `coordinator_ended_by_policy{from_teacher_id, student_count: 0}` |
| `teacher_load` (öğretmen) | `load_set{base_hours, max_extra_hours, other_extra_hours, chief_type, employment_type, previous?, source}` |
| `teacher_schedule` (öğretmen) | `schedule_set{slots, previous_slot_count?, source}` |
| her akış | `revoked` (yük `{}`, hedefi `revokes` sütununda) |

**`hours_capped` bir anlık görüntü değil, sınırlamadır.**
- Katlamada şöyle uygulanır: `awarded := min(önceki.awarded, cap)` ve `max_hours_snapshot := min(önceki.max_hours_snapshot, cap)`. Diğer alanlar önceki durumdan taşınır.
- **Katlama anında** o tarihte `is_locked = 1` ise sınırlama uygulanmaz. Böylece sonradan geçmiş tarihle girilen bir kilit doğru sonuç verir.
- Politikanın çıktısı olay olarak saklanır ve katlama sırasında yeniden türetilmez. Bu sayede tavan yükseldiğinde saat kendiliğinden geri gelmez.

**Sürümleme:**
- Yeni alan eklemek için `#[serde(default)]` kullanılır; sürüm artırılmaz.
- Kırıcı bir değişiklikte `XxxV1` yapısı saklanır, `From<XxxV1>` yazılır ve `decode` içine yeni bir kol eklenir.
- Her `(kind, version)` için bir golden JSON fixture'ı sonsuza kadar decode edilebilmelidir (MESNET #137 dersi).

## 5. Yazma yolu

**Tek kapı:** `services/change_service.rs::execute_change(pool, req, mode, today)`. Mod `Preview` ya da `Commit { expected_high_water }` olabilir.

Adımların hepsi `pool.begin_with("BEGIN IMMEDIATE")` ile açılan tek transaction içinde ve **aynı bağlantı** üzerinden çalışır. Transaction içinde havuz kullanmak yasaktır: havuzdan okuma sessizce eski veriyi döndürür, havuza yazma 5 saniye sonra "database is locked" hatası verir.

1. **High-water kontrolü.** `MAX(change_events.id)` önizlemeden bu yana değiştiyse `Stale` döner.
2. **Yerinde oluşturma.** Gerekirse `companies::create_in`, `students::create_in` ya da `teachers::create_in` çağrılır.
3. **Bağlam.** `history_context::load(conn, term, today)` dönemin tüm olaylarını ve kurallarını okur.
4. **Karar.** `decide(&ctx, &req) -> Result<Decision, Rejection>` saf fonksiyonu çalışır. Çıktısı: birincil olaylar, `caused_by` ile bağlı politika olayları, `ImpactSummary` ve dokunulan akış anahtarları.
5. **Günlüğe ekleme.** `change_log::append_change_set` ve `append_events` çağrılır. `caused_by` indeksleri bu adımda gerçek id'ye çevrilir.
6. **Projeksiyon.** Dokunulan her `(stream, subject, term)` için canlı olaylar `(effective_date, id)` sırasıyla katlanır. Aynı günde çakışan olaylarda sonra kaydedilen kazanır, sıfır uzunluklu aralık atılır. O öznenin satırları silinir ve yeniden yazılır. **Rebuild olay başına değil, özne başına bir kez** yapılır.
7. **Commit.** Herhangi bir hata olursa hiçbir şey yazılmaz. Yerinde oluşturulan işletme de geri alınır.

Önizleme aynı `decide` fonksiyonudur, yalnız hiçbir şey yazmaz. Onaylanan özet `impact_json` alanına kaydedilir.

### 5.1 Tarih kuralları (`domain/terms.rs`)

- **Planlama** (`today < start_date`): tarih boş bırakılabilir, boşsa `start_date` kullanılır.
- **Dönem başladıysa:** tarih zorunludur. Tarih şu aralıkta olmalıdır: `[max(start_date, bugünün ayının 1'i), end_date]`.
- Önceki aya düşen tarih için `Rejection{code: PreviousMonthClosed, suggested_date: bugünün ayının 1'i}` döner. Kullanıcı önerilen tarihi seçerse gerçek tarih `document_date` alanına yazılır.
- **Açılış olayları dönem başı tarihini kilitlemez.** Katlama, `opening` türündeki change set'lerin olaylarını saklanan tarihlerine değil, **`terms.start_date` tarihine** yerleştirir. Saklanan tarih yalnız bilgi amaçlıdır; günlük değişmez kalır.
- **`update_term_dates` kuralları:**
  - Bugünden önceki bir ayın gün kümesini değiştiren tarih değişikliği reddedilir.
  - Başlangıcı, açılış **dışındaki** bir olayın yürürlük tarihinden sonraya almak reddedilir.
  - Bitişi, herhangi bir olayın yürürlük tarihinden önceye almak reddedilir.
  - Kabul edilen değişiklikten sonra, aynı transaction içinde `projection::rebuild_term` çalışır.
- `today` sistem saatinden alınır ve `Europe/Istanbul` yerel tarihi olarak hesaplanır. `recorded_at` UTC'dir.

### 5.2 Geçmiş tarihli giriş

- Birincil olay, yürürlük tarihinin hemen öncesindeki durumu (`d⁻`) okur. Olgu o tarihte doğru değilse komut reddedilir (ör. "Öğrenci 3 Kasım'da A işletmesinde değil").
- Aday olay zaman çizelgesine yerleştirilir ve beklenen önceki değer taşıyan **sonraki** olaylar yeniden doğrulanır. İlk çelişki komutu `ConflictsWithLaterChange` ile reddeder. Mesaj çelişen change set'i adıyla verir ve `conflicting_change_set_ids` döner. **Olaylar otomatik olarak yeniden sıralanmaz.**
- Anlık görüntü akışlarında (saat, yük, program) değişiklik, o öznenin bir sonraki kaydına kadar geçerli olur. Bunun için `shadowed` bildirimi verilir.
- Elle girilen saat, sunucunun o tarih için hesapladığı tavanı **aşacak şekilde artırılamaz**. Kilitli ve tavanın üstünde kalmış bir satırın notu değiştirilebilir, kilidi kaldırılabilir, saati düşürülebilir.
- İstemcinin gönderdiği `maxHoursSnapshot` artık kullanılmaz; tavan sunucuda hesaplanır.

### 5.3 Zincir etki (`domain/history/policy.rs`, saf)

**Etkilenen işletmeler:** eski ve yeni işletme, yerinde oluşturulan işletme, geri alınan olaylarda adı geçen işletmeler.

Her etkilenen işletme için `d` tarihinden dönem sonuna kadar **her değişiklik tarihi** `b` dolaşılır:

- `n = öğrenci_sayısı(b)`.
- Tavan:
  - `n = 0` ise 0;
  - değilse `select_narrowest(rules, round_trip_km, n).max_hours`;
  - mesafe ya da kural bilinmiyorsa `None`.
- Saat tavanın üstündeyse ve satır fahri değilse:
  - kilitli satırda `lockedAboveCap` uyarısı;
  - değilse `hours_capped` (`caused_by` = kök olay).
- Tavan `None` ve saat 0'dan büyükse `capUnknown` uyarısı.
- `n = 0` ve koordinatör varsa `coordinator_ended_by_policy`.
- Tavan yükseldiyse `capIncreased` bildirimi.
- `awarded < cap` durumunda o saati daha önce bir `hours_capped` düşürmüşse `reducedBelowCap` bildirimi: "otomatik düşürülmüş, şimdi tavanın altında; elle geri verebilirsiniz". Bu, iki ayrılıştan birinin geri alınması gibi çok nedenli durumları kapsar.
- Yerinde oluşturulan işletme için `newCompanyNeedsSetup` bildirimi.

**Öğretmen tarafı** (her değişiklik tarihinde):
- `workload::coordinator_capacity` ve `validation::check_teacher_totals` (günlük 8 saat) **yeniden kullanılır**; kural yeniden yazılmaz.
- İhlaller `[from, to)` aralıklı uyarılara dönüşür: `capacityExceeded`, `dailyCapExceeded`.
- Program değişince boş saatin dışında kalan, zorlanmamış bloklar için `blockOutsideFreeSlots` uyarısı.
- Aynı öğretmende tarih aralıkları örtüşen blok çakışması **reddedilir**.

**Kapasite hesabının tek yeri:** Bugün `teacher_commands.rs`, `assignment_commands.rs` ve `dashboard_commands.rs` bu türetmeyi ayrı ayrı yapıyor. Tarih eklenmeden önce bu türetme, tarihi dikkate alan tek bir fonksiyonda birleştirilir.

### 5.4 Geri alma ve düzeltme

- **Geri alma** change set birimiyle yapılır. Hedef kümedeki her canlı olay için bir `revoked` olayı yazılır. Hedef kümenin kendi zincir olayları da geri alınır.
- Başka bir kayıt buna bağlıysa komut reddedilir: "Önce #57'yi geri alın". Geri almalar en yeniden eskiye doğru yapılır.
- **Geri alınamayanlar:**
  - açılış kümeleri,
  - geri alma kümeleri,
  - yürürlük tarihi önceki aya düşen kümeler.
- **Düzeltme:** `{type:'correct', changeSetId, replacement}`. Geri alma ve yeni giriş **tek change set** içinde yapılır: tek önizleme, tek commit, ara durum yok.
- Yerinde oluşturulmuş bir işletmeye yapılan nakil geri alınırsa ve işletmenin başka referansı yoksa `companies.is_active = 0` olur.
- Olaylarda ya da projeksiyonlarda geçen öğrenci, öğretmen veya işletme **silinemez**. Mesajda pasif yapma önerilir: "Bu kaydın geçmişi var; silinemez, pasif yapabilirsiniz".
- `assignments::clear_term` ve otomatik dağıtım önerisini uygulama yalnız planlama evresinde çalışır.

## 6. Okuma yolu

- **As-of koşulu:** `valid_from <= ?d AND (valid_to IS NULL OR ?d < valid_to)`. Varsayılan `?d`, bugünün dönem aralığına sıkıştırılmış hâlidir.
- Okuma komutları `asOf: string | null` alır. Yanıtlar `asOf` ve `isPlanning` alanlarını döndürür.
- Görev çizelgesi (PDF), ziyaret listesi, Excel ve Genel Bakış `asOf` ile üretilir.
- Öğrenci DTO'sunda camelCase alan adı `companyId` olarak kalır; değeri artık yerleşim projeksiyonundan gelir. Yeni alan: `placementFrom`.
- **Denetim listesi:** `change_sets` + `change_events`.
  - Filtreler (öğrenci, işletme, öğretmen) `LIMIT`'ten **önce** uygulanır.
  - "Önce" değeri, katlamanın `(d, id)` öncesindeki durumudur.
- **Kayıt zamanı sorgusu** ("R anında ne biliyorduk") Faz A arayüzünde yoktur. Yapısal olarak mümkündür: `id <= X` ile aynı katlama.

## 7. Arayüz

| Ekran | Değişiklik |
|---|---|
| Öğrenciler | Yeni **Nakil / Ayrılış** penceresi. Hedef işletme mevcut listeden seçilir ya da yerinde oluşturulur (`CompanyFormDialog`). Tarih etiketi olaya göre değişir. `StudentFormDialog` içindeki işletme alanı yalnız ilk yerleştirme içindir. |
| Öğretmenler | Yeni **Yük değişikliği** penceresi: ders saati, şeflik, kadro + geçerlilik tarihi |
| Müsaitlik | Dönem başladıysa kaydederken tarih sorulur |
| Dağıtım (`AllocationView`) | Koordinatör değişikliği → tarih → etki penceresi. **Bu fazda bölünmez**; tarih sorusu ayrı bir bileşendir, net büyüme en fazla 20 satırdır. |
| Saat Ayarları | Dönem başladıysa tarih + etki penceresi |
| **Etki penceresi** (ortak) | Birincil / Otomatik / Uyarılar / Bildirimler. `rejected` sonucunda onay düğmesi kapalıdır; `stale` sonucunda yeniden önizleme yapılır. |
| **Tarihçe** (yeni) | Değişiklik kümeleri; otomatik alt olaylar iç içe gösterilir, geri alınanlar üstü çizilir, filtreler vardır. "Geri al" ve "Düzelt" yalnız pencere içindeki kayıtlarda etkindir. |
| Dönem Yönetimi | Dönem başı ve sonu, "onaylandı" işareti. Onay verilene kadar uyarı başlığı gösterilir. |
| Sidebar | "Tarihteki durum" seçicisi. Bugün dışında bir tarih seçilirse ekranlar salt okunur olur. |

Tüm yeni metinler `src/i18n/labels.ts` içinde yer alır. Olay türü etiketleri:

- Yerleştirme, Nakil, İşletmeden ayrılış
- Saat takdiri, Tavan düşüşü (otomatik), Saat kaldırıldı
- Koordinatör atama, Koordinatörlük bitti, Koordinatörlük bitti (öğrenci kalmadı)
- Öğretmen yükü, Haftalık program, Geri alındı

## 8. Tauri sözleşmesi (camelCase, birebir)

**Komutlar:**
- `preview_change({ request })`
- `commit_change({ request, expectedHighWater: number | null })`

**İstek:**
```ts
request = {
  term: string,
  effectiveDate: 'YYYY-MM-DD' | null,
  documentDate: 'YYYY-MM-DD' | null,
  reason: string,
  command: ChangeCommand,
}
```

**`command` türleri** (`type` etiketli):

| `type` | Alanlar |
|---|---|
| `createStudent` | `student`, `companyId \| null` |
| `placeStudent` | `studentId`, `companyId` |
| `transferStudent` | `studentId`, `fromCompanyId`, `to: {type:'existing', companyId} \| {type:'new', company: NewCompany}` |
| `studentLeaves` | `studentId`, `fromCompanyId` |
| `deleteStudent` | `studentId` |
| `setCompanyHours` | `rows: [{companyId, awardedHours, isHonorary, isLocked, notes}]` |
| `assignCoordinators` | `rows: [{companyId, teacherId, visitDay, visitHour, isForced, forceReason}]` |
| `endCoordination` | `companyId` |
| `clearCoordination` | — |
| `createTeacher` | `teacher`, `load` |
| `setTeacherLoad` | `teacherId`, `load: {baseHours, maxExtraHours, otherExtraHours, chiefType, employmentType}` |
| `setTeacherSchedule` | `teacherId`, `slots: [{dayOfWeek, hour}]` |
| `copySchedulesFromTerm` | `fromTerm` |
| `deleteTeacher` | `teacherId` |
| `revoke` | `changeSetId` |
| `correct` | `changeSetId`, `replacement: <yukarıdakilerden biri>` |

**`ChangeOutcome`:**

| `status` | Alanlar |
|---|---|
| `rejected` | `code`, `reason`, `conflictingChangeSetIds`, `suggestedDate \| null` |
| `stale` | `message` |
| `preview` | `impact`, `highWater` |
| `committed` | `changeSetId`, `impact` |

**`rejected.code` değerleri:**

| `code` | Anlamı |
|---|---|
| `previousMonthClosed` | Tarih önceki aya düşüyor. `suggestedDate` = bu ayın 1'i. |
| `effectiveDateRequired` | Dönem başladı, tarih girilmedi. |
| `outOfTerm` | Tarih dönem aralığının dışında. |
| `factNotTrueAtDate` | Ör. öğrenci o tarihte o işletmede değil. |
| `conflictsWithLaterChange` | Sonraki bir kayıtla çelişiyor. `conflictingChangeSetIds` dolu gelir. |
| `aboveCap` | Elle girilen artış, tavanı aşıyor. |
| `blockOverlap` | Aynı öğretmende tarihleri örtüşen blok var. |
| `notRevocable` | Açılış kümesi, geri alma kümesi ya da önceki aya düşen küme. |
| `hasDependents` | Önce sonraki bağlı kayıt geri alınmalı. `conflictingChangeSetIds` dolu gelir. |
| `hasHistory` | Kaydın geçmişi var, silinemez; pasif yapılabilir. |
| `planningOnly` | Dönem başladı, bu işlem yalnız planlamada yapılabilir. |
| `invalidRequest` | Komuttaki öğrenci, işletme, öğretmen ya da değişiklik kümesi yok; işletme pasif; ya da yerinde oluşturulması gereken satır oluşturulmamış. |

**`ImpactSummary`:** `{effectiveDate, isPlanning, shadowedUntil | null, primary, automatic, warnings, notices}`

- `ImpactLine`: `{kind, stream, subjectId, subjectLabel, effectiveDate, before, after}`
- `ImpactWarning`: `{code, message, subjectLabel, fromDate, toDate | null}`
  - `code`: `lockedAboveCap` | `capacityExceeded` | `dailyCapExceeded` | `blockOutsideFreeSlots` | `capUnknown`
- `ImpactNotice`: `{code, message, subjectLabel, date}`
  - `code`: `capIncreased` | `reducedBelowCap` | `newCompanyNeedsSetup` | `futureDated` | `shadowed`

**Tarihçe:**
- İstek: `list_history({ filter: {term, stream | null, subjectId | null, companyId | null, teacherId | null, includeOpening, beforeChangeSetId | null, limit} })`
- Yanıt:
  ```ts
  {
    entries: [{
      changeSetId, recordedAt, kind, reason, actor,
      effectiveDate, documentDate,
      revokedByChangeSetId | null, revokesChangeSetId | null,
      isRevocable, warnings,
      events: [{eventId, stream, subjectId, subjectLabel, kind,
                effectiveDate, before, after, causedByEventId | null, isRevoked}],
    }],
    nextBeforeChangeSetId | null,
  }
  ```

- `get_subject_history({stream, subjectId, term})` → `HistoryEventEntry[]`. Şekli, yukarıdaki `events[]` öğesiyle aynıdır.

**Dönemler:**
- `list_terms_with_dates()` → `[{term, startDate, endDate, datesConfirmed, isPlanning, defaultAsOf, earliestAllowedDate}]`
- `update_term_dates({term, startDate, endDate, confirm})` → güncellenmiş kaydı döndürür (`TermWithDates`, `list_terms_with_dates` öğesiyle aynı şekil).

**İç gövdeler** (V1'de sabitlendi):
- `createStudent.student` = `NewStudentInput`: `{firstName, lastName, studentNo | null, grade, branch, submittedAt | null}`.
  - `companyId` ve `term` bu gövdede **yoktur**; zarftaki `companyId` ve istekteki `term` kullanılır.
- `createTeacher.teacher` = `{firstName, lastName, registryNo, field, branches: string[], isActive}`.
- `createTeacher.load` = `setTeacherLoad.load` ile aynı şekildedir.
- `correct.replacement` herhangi bir `ChangeCommand` olabilir. Ancak `revoke` ve `correct`, `decide` tarafından `notRevocable` ile reddedilir.

## 9. Göç

**Başlamadan önce:** Kullanıcının `mesnet-lite.db` dosyasının yedeği alınır. Göç geri alınamaz.

**`0006`** (sqlx her göçü kendi transaction'ında çalıştırır):

1. **`terms` satırları.** Bugünkü `known_terms` birleşiminden türetilir. Varsayılan tarihler yazılır, `dates_confirmed = 0`. Biçimi bozuk eski dönemler `2000-01-01..2099-12-31` alır.
2. **Açılış kümesi.** Her dönem için `opening` türünde bir change set: `effective_date = start_date`, `actor = 'migration'`.
3. **Açılış olayları.** Mevcut `students.company_id`, `company_term_hours`, `assignments`, öğretmen yük sütunları (dönem × öğretmen) ve `teacher_availability` (`json_group_array` ile) açılış olaylarına çevrilir.
   - Aktif dönemin yükü için `source = 'opening'`, eski dönemler için `'opening_assumed'`.
4. **Projeksiyonlar.** Açılış olaylarından `[start_date, NULL)` aralıklı satırlar üretilir.
5. **`companies.is_active`** ve **`operator_name`** eklenir.
6. Eski tablolar yerinde kalır.

**Yeni dönem oluşturma:** `create_term` bir `terms` satırı ve bir açılış kümesi yazar. Her aktif öğretmenin son `load_set` kaydı kopyalanır (`source = 'copied'`). Program kopyalama isteğe bağlıdır ve `copySchedulesFromTerm` ile yapılır.

**`0007`** (R6 biriminden sonra):
- `assignments`, `company_term_hours` ve `teacher_availability` DROP edilir.
- `idx_students_company` ve `students.company_id` DROP edilir.
- Öğretmen tablosundaki 5 yük sütunu `ALTER TABLE … DROP COLUMN` ile kaldırılır.
- **Ana tablolar (`students`, `teachers`, `companies`) yeniden kurulmaz.** FK açıkken `DROP TABLE` örtük bir DELETE yapar ve CASCADE ile geçmişi siler.

## 10. Testler

**Test önce yazılır.** Mevcut testler her birimden sonra yeşil kalmalıdır.

**Saf testler:**
- `timeline`: yarı açık aralıklar; aynı günde sonra kaydedilen kazanır; `None` boşluk bırakır; `valid_to` günü dahil değildir.
- `events`: her `(kind, version)` için golden JSON; encode/decode gidiş-dönüşü; bilinmeyen tür panic değil hata döndürür.
- `decide`:
  - önceki ay reddi ve `suggested_date`;
  - planlamada tarih varsayılanı;
  - sonraki kayıtla çelişki reddi (çelişen id ile);
  - tavanı aşan artış reddi;
  - kilitli satırın düzenlenebilmesi;
  - tarih örtüşmesine bağlı blok çakışması;
  - geri alma bağımlılığı;
  - düzeltmenin tek küme olması.
- `policy`:
  - tavana indirme;
  - kilitli satırda yalnız uyarı;
  - geçmiş tarihli kilidin sınırlamayı engellemesi;
  - 0 öğrencide koordinatörün bitmesi ve saatin 0 olması;
  - tavan artışında yalnız bildirim;
  - `reducedBelowCap`;
  - kapasite bayrağı (24/10/4 → 10);
  - boş saat dışındaki blok bayrağı.

**Özellik testleri** (yeni bağımlılık yok; tüm permütasyonlar ve `#[cfg(test)]` içinde tohumlu LCG):
- **P1:** Katlama çıktısında çakışma yok, sıralı, her `source_event_id` bir kez geçiyor.
- **P2:** Farklı `(date, id)` değerlerinin kayıt sırası karıştırılınca sonuç aynı.
- **P3:** `state_at(x)`, tarihi `x` ya da öncesi olan son adımın durumuna eşit.
- **P4 (ay değişmezliği):** Yürürlük tarihi ≥ ayın 1'i olan herhangi bir olay, önceki ayların hiçbir `state_at` değerini değiştirmiyor.
- **P5:** `e` olayını geri almak, `e` ve onun `caused_by` çocukları hiç yokmuş gibi katlamakla aynı sonucu veriyor.

**DB testleri:**
- `0006` göçü, `0005` noktasına kadar kurulmuş dolu bir fixture üzerinde çalıştırılır. Sayılar tutmalı, `verify_term` sapma bulmamalı, tüm yükler decode edilebilmeli.
- Günlükte UPDATE ve DELETE reddedilmeli.
- Önizleme hiçbir şey yazmamalı.
- Commit atomik olmalı: projeksiyonda FK hatası olursa change set de yerinde oluşturulan işletme de kalmamalı.
- `Stale` dönüşü çalışmalı.
- `max_connections(1)` havuzuyla yerinde oluşturmalı nakil ve CSV içe aktarma çalışmalı. Transaction içinde havuz kullanılırsa test zaman aşımıyla düşer.
- **P6:** Rastgele komut dizileri çalıştırılır. Her commit'ten sonra `verify_term` (replay = projeksiyon) ve çakışma sorgusu 0 satır dönmeli.
- Her karar için bir senaryo testi.

**Kapılar:**
- `cargo test` ve `cargo check` (`src-tauri`);
- `npm run build` ve `npx vue-tsc --noEmit`;
- orkestratör, **gerçek veritabanının bir kopyası** üzerinde `verify_term` çalıştırır.

## 11. Kapsam dışı (Faz A)

- Event sourcing crate'i, bus, saga, outbox, snapshot, UUID, akış başına sıra numarası, `proptest` / `fastrand`.
- Kayıt zamanı sorgusunun arayüzü.
- Kural tablosu, işletme mesafesi ve kurum tipi tarihçesi. Kullanılan tavan olayda dondurulduğu için gerekmiyor.
- `AllocationView` bölünmesi. 1189 satır, 800 sınırının üstünde; ayrı bir iş olarak kalır.

## 12. Faz B'ye bırakılanlar

- Aylık ek ders puantajı: `(dönem günleri × ziyaret günü × o gün geçerli saat)`, tatil ve iş günü dışı takvim (`non_working_days`).
- İlçeye gönderilen puantajın o anki çıktısının değişmez bir kaydı. Kural tablosu ya da kod değişirse geçmiş ayın yeniden hesabı kaymasın diye.
- Faz A bunun için hazır: ay penceresi, geçmiş ayı olay düzeyinde zaten değişmez kılıyor.
