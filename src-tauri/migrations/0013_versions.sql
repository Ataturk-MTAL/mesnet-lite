-- Kayıtlı sürümler: veritabanının belirli bir andaki TAM kopyası. Kullanıcı
-- kararı — önceki durumların çıktısı yalnızca geçmişi (change_sets/
-- change_events) yeniden oynatarak değil, o anın dosyasını saklayarak alınır;
-- bir sürüm seçilince o günkü hâliyle birebir beş çıktıdan biri üretilebilir.
--
-- Bir satır, `services::versions::create_version`in `VACUUM INTO` ile
-- `<app_data_dir>/versions/` altına yazdığı bir dosyanın izidir; satır
-- silinmeden dosya, dosya silinmeden satır anlamsız kalır — ikisi birlikte
-- `services::versions::delete_version`te silinir.
PRAGMA foreign_keys = ON;

CREATE TABLE versions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    -- 'auto': beş dışa aktarım komutundan biri her çağrıldığında (üretim
    -- başarılıysa) kendiliğinden alınır. 'manual': kullanıcı bir ad vererek
    -- elle kaydeder. `trigger` yalnızca 'auto' satırlarında doludur.
    kind        TEXT NOT NULL CHECK (kind IN ('auto', 'manual')),
    -- Otomatik sürümde hangi çıktının tetiklediğini taşır (ör.
    -- 'assignmentSheet', 'visitLists', 'commissionMinutesPdf',
    -- 'commissionMinutesXlsx', 'workbook'); elle kayıtta NULL.
    trigger     TEXT,
    -- Sürüm alındığı anda aktif olan dönem; listede kullanıcıya "bu sürüm
    -- hangi döneme ait" bilgisini gösterir.
    term        TEXT NOT NULL,
    -- `versions/` klasöründeki dosya adı. UNIQUE: iki satır aynı dosyayı
    -- göstermez, bu yüzden birini silmek diğerinin verisini yok etmez.
    file_name   TEXT NOT NULL UNIQUE,
    -- `versions`/`_sqlx_migrations` dışındaki tabloların deterministik
    -- (tablo adı, sonra rowid sırasıyla) içerik özeti (SHA-256). Otomatik
    -- sürümde ardışık iki çağrının veri DEĞİŞMEDİĞİNİ anlaması için.
    fingerprint TEXT NOT NULL,
    -- Türkiye yerel saatiyle (`domain::terms::now_local`, sabit +03:00)
    -- 'YYYY-MM-DDTHH:MM:SS'; listede en yeniden eskiye sıralamak için `id`
    -- kullanılır (ekleme sırasıyla birebir örtüşür), bu sütun yalnız
    -- gösterim amaçlıdır.
    created_at  TEXT NOT NULL
) STRICT;
