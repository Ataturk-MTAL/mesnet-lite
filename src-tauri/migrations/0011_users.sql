-- Kayıt tutma amaçlı kullanıcı girişi: 2-3 kişilik, hepsi TAM YETKİLİ bir
-- ekip için "kim yaptı" bilgisini tarihçeye düşürür. Amaç yetkilendirme
-- DEĞİLDİR — bu masaüstü uygulaması korumasız bir SQLite dosyasına yazar,
-- makineye erişen dosyayı zaten açabilir. PIN yine de ÖZETLENEREK saklanır
-- (bkz. Cargo.toml'daki argon2 bağımlılığının yorumu): zayıf bir sınır bile
-- olsa açık parola saklamak ayrı, önlenebilir bir hatadır.
--
-- Kasıtlı olarak yapılmayan: deneme sayacı, hesap kilitleme, oturum zaman
-- aşımı. Bunlar aşılabilir bir sınırı savunmaya çalışır ve üç kişilik tek
-- makine senaryosunda gereksiz karmaşadır.
CREATE TABLE users (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    pin_hash   TEXT    NOT NULL,
    is_active  INTEGER NOT NULL DEFAULT 1,
    created_at TEXT    NOT NULL
) STRICT;

CREATE UNIQUE INDEX idx_users_name ON users(name);

-- Tohum kullanıcı YOK: sabit kodlanmış bir varsayılan PIN gerçek bir
-- zayıflık olurdu. Tablo boş başlar, ilk kullanıcıyı arayüz
-- (`has_any_user` false döndüğünde gösterdiği kurulum ekranı) oluşturur.

-- `settings.operator_name` (bkz. migration 0006) SATIRI KALDIRILMADI; anlamı
-- genişledi. `change_service.rs` bu ayarı `change_sets.actor`a yazarken hâlâ
-- doğrudan okur — değişikliği kimin yaptığının tarihçeye düşmesi bu satıra
-- bağlı; artık değeri elle girilen serbest metin değil, `login` komutunun PIN
-- doğrulaması SONRASI yazdığı "o an giriş yapmış kullanıcının adı"dır.
