use crate::db::companies::normalize_name;
use crate::db::settings;
use crate::error::{AppError, AppResult};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::Serialize;
use sqlx::SqlitePool;

/// PIN uzunluk sınırları: 4 haneden kısası tahmin edilmesi kolay bir sınır
/// olur, 6 haneden uzunu üç kişilik tek makine için gereksiz zorluktur —
/// bu uygulamanın PIN'i yetkilendirme değil KAYIT TUTMA amaçlıdır (bkz.
/// migration 0011 başlığı), o yüzden aralık cömerttir.
const PIN_MIN_LEN: usize = 4;
const PIN_MAX_LEN: usize = 6;

const SELECT_COLUMNS: &str = "id, name, is_active";

/// Arayüze dönen kullanıcı özeti. `pin_hash` KASITLI OLARAK burada YOKTUR:
/// özet biçimde bile olsa PIN'in herhangi bir türevi arayüze/loglara sızmamalı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn not_found(id: i64) -> AppError {
    AppError::NotFound(format!("Kullanıcı bulunamadı: {id}"))
}

/// PIN biçimini doğrular: yalnız rakam, 4-6 hane. Sınırda doğrulama —
/// `create_user` ve `set_user_pin` çağrılmadan önce burada yapılır.
fn validate_pin_format(pin: &str) -> AppResult<()> {
    let invalid = || {
        AppError::Validation(
            "PIN yalnızca rakamlardan oluşmalı ve 4-6 hane olmalı".into(),
        )
    };
    if pin.len() < PIN_MIN_LEN || pin.len() > PIN_MAX_LEN {
        return Err(invalid());
    }
    if !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }
    Ok(())
}

/// İsmi kırpar ve boş olmadığını doğrular. Büyük/küçük harf duyarsız
/// benzersizlik denetimi ayrı bir adımdır (çağıran mevcut listeyle karşılaştırır).
fn validate_name(name: &str) -> AppResult<String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Kullanıcı adı boş olamaz".into()));
    }
    Ok(trimmed)
}

/// PIN'i özetler. Varsayılan Argon2id parametreleri kullanılır — brief kararı:
/// "varsayılan parametreler yeterli, ayarlarla oynama".
fn hash_pin(pin: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::Validation(format!("PIN özetlenemedi: {e}")))
}

/// Verilen PIN'in özetle eşleştiğini doğrular. Bozuk bir özet (elle bozulmuş
/// veritabanı gibi bir uç durum) hata değil, "eşleşmedi" sayılır — `login`
/// komutunun "kullanıcı yok" ile "PIN yanlış" ayrımı yapmama kuralıyla tutarlı.
fn verify_pin(pin_hash: &str, pin: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(pin_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(pin.as_bytes(), &parsed)
        .is_ok()
}

/// Aynı ada (büyük/küçük harf duyarsız, boşluk normalize edilmiş) sahip
/// AKTİF VEYA PASİF başka bir kullanıcı var mı? Pasif kullanıcılar da
/// çakışmaya dahildir: yumuşak silinmiş bir isim yeniden kullanılamaz,
/// yoksa tarihçedeki eski `actor` değeriyle yeni kullanıcı karışabilir.
async fn name_conflicts(pool: &SqlitePool, name: &str, exclude_id: Option<i64>) -> AppResult<bool> {
    let target = normalize_name(name);
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM users")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .any(|(id, existing)| Some(id) != exclude_id && normalize_name(&existing) == target))
}

fn name_conflict_error() -> AppError {
    AppError::Validation("Bu isimde bir kullanıcı zaten var".into())
}

/// En az bir kullanıcı var mı? Arayüz bu `false` döndüğünde ilk kullanıcıyı
/// kuran kurulum ekranını gösterir (tohum kullanıcı BİLİNÇLİ OLARAK yok,
/// bkz. migration 0011).
pub async fn has_any_user(pool: &SqlitePool) -> AppResult<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

/// Pasifler dahil TÜM kullanıcılar. Giriş ekranı yalnız `isActive: true`
/// olanları göstermeli — bu süzme arayüz tarafının işidir, burada değil.
pub async fn list(pool: &SqlitePool) -> AppResult<Vec<UserSummary>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM users ORDER BY name COLLATE NOCASE");
    Ok(sqlx::query_as::<_, UserSummary>(&sql).fetch_all(pool).await?)
}

async fn get_summary(pool: &SqlitePool, id: i64) -> AppResult<UserSummary> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM users WHERE id = ?1");
    sqlx::query_as::<_, UserSummary>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| not_found(id))
}

pub async fn create(pool: &SqlitePool, name: &str, pin: &str) -> AppResult<UserSummary> {
    let name = validate_name(name)?;
    validate_pin_format(pin)?;
    if name_conflicts(pool, &name, None).await? {
        return Err(name_conflict_error());
    }
    let pin_hash = hash_pin(pin)?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (name, pin_hash, is_active, created_at)
         VALUES (?1, ?2, 1, ?3)
         RETURNING id",
    )
    .bind(&name)
    .bind(pin_hash)
    .bind(now_iso())
    .fetch_one(pool)
    .await?;

    get_summary(pool, id).await
}

/// Kullanıcıyı yeniden adlandırır ve — yalnızca eski ad oturumdaki kullanıcıya
/// aitse — `settings.operator_name`i tazeler. Bu ikinci adım olmadan, oturum
/// açmış kullanıcı kendi adını değiştirdiğinde `change_set.actor` çıkış
/// yapılana kadar artık hiçbir kullanıcıyla eşleşmeyen ESKİ adla yazılmaya
/// devam ederdi (teşhis edilen kusur).
pub async fn rename(pool: &SqlitePool, id: i64, name: &str) -> AppResult<()> {
    let name = validate_name(name)?;
    // Çakışma denetimi havuzdan okur; transaction içindeyken havuzdan okumak
    // yasaktır (bkz. `companies::create_in` üstündeki yorum — açık bir
    // `BEGIN IMMEDIATE` sırasında havuz sessizce eski veri döndürebilir), bu
    // yüzden transaction başlamadan ÖNCE, burada yapılır.
    if name_conflicts(pool, &name, Some(id)).await? {
        return Err(name_conflict_error());
    }

    // Eski adı okuma, kullanıcıyı güncelleme ve `operator_name`i tazeleme
    // TEK `BEGIN IMMEDIATE` transaction'ında: aradaki okumalar başka bir
    // yazıcıyla yarışmasın diye (spec genelindeki `BEGIN IMMEDIATE` kuralıyla
    // aynı gerekçe, bkz. `services/change_service.rs` başlığı).
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    let old_name: Option<String> = sqlx::query_scalar("SELECT name FROM users WHERE id = ?1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(old_name) = old_name else {
        return Err(not_found(id));
    };

    let affected = sqlx::query("UPDATE users SET name = ?1 WHERE id = ?2")
        .bind(&name)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(not_found(id));
    }

    // `operator_name` YALNIZ eski ad bu kullanıcıya aitse tazelenir; başka
    // bir kullanıcıya aitse (o kullanıcı oturumda demektir) dokunulmaz —
    // brief sözleşmesi. Karşılaştırma `name_conflicts`teki AYNI normalize
    // mantığını (küçük harf + boşluk sıkıştırma) kullanır: adlar benzersiz
    // olduğu için bu, `operator_name`in tam olarak BU kullanıcıya ait olup
    // olmadığını güvenilir biçimde söyler.
    let operator_name: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'operator_name'")
            .fetch_optional(&mut *tx)
            .await?;
    let operator_is_this_user = operator_name
        .as_deref()
        .map(|current| normalize_name(current) == normalize_name(&old_name))
        .unwrap_or(false);
    if operator_is_this_user {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ('operator_name', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn set_pin(pool: &SqlitePool, id: i64, pin: &str) -> AppResult<()> {
    validate_pin_format(pin)?;
    let pin_hash = hash_pin(pin)?;

    let affected = sqlx::query("UPDATE users SET pin_hash = ?1 WHERE id = ?2")
        .bind(pin_hash)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

/// Yumuşak silme: `is_active` bayrağını değiştirir, satırı SİLMEZ — bu
/// projenin yaklaşımı (bkz. `companies`/`teachers`) ve tarihçedeki eski
/// `actor` adının düz metin olarak durmaya devam etmesi buna bağlıdır.
///
/// Pasifleştirme (yalnız `is_active = false` yönü) son etkin kullanıcıyı
/// reddeder: giriş ekranı yalnız aktifleri listeler, aktif sayısı sıfıra
/// düşerse ekran boş kalır ve uygulama veritabanı elle düzeltilmeden
/// açılamaz hâle gelir. Aktifleştirmenin (`true`) böyle bir riski yoktur.
pub async fn set_active(pool: &SqlitePool, id: i64, is_active: bool) -> AppResult<()> {
    if !is_active {
        let other_active: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE is_active = 1 AND id <> ?1",
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        if other_active == 0 {
            return Err(AppError::Validation(
                "Son etkin kullanıcı pasife alınamaz; önce başka bir kullanıcı ekleyin.".into(),
            ));
        }
    }

    let affected = sqlx::query("UPDATE users SET is_active = ?1 WHERE id = ?2")
        .bind(is_active)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

/// PIN doğruysa `settings.operator_name`i bu kullanıcının adına yazar ve
/// `true` döner; yanlışsa HİÇBİR ŞEY YAZMAZ ve `false` döner. "Kullanıcı yok"
/// ile "PIN yanlış" arasında AYRIM YAPILMAZ (brief kararı) — kullanıcı zaten
/// listeden seçiliyor, ayrım gereksiz bir bilgi sızıntısı olurdu.
///
/// Pasif kullanıcı PIN doğru olsa bile giremez — bu kural burada, kapıda
/// uygulanır. Giriş ekranı yalnız aktifleri listeleyecek olsa da kimlik
/// elle (ör. doğrudan komut çağrısıyla) gönderilebilir; kuralı yalnız
/// arayüzün filtresine bırakmak burada fiilen uygulanmaması demektir.
pub async fn login(pool: &SqlitePool, user_id: i64, pin: &str) -> AppResult<bool> {
    #[derive(sqlx::FromRow)]
    struct Row {
        pin_hash: String,
        is_active: bool,
    }

    let row: Option<Row> = sqlx::query_as("SELECT pin_hash, is_active FROM users WHERE id = ?1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        return Ok(false);
    };
    if !row.is_active {
        return Ok(false);
    }
    if !verify_pin(&row.pin_hash, pin) {
        return Ok(false);
    }

    let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id = ?1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    settings::set(pool, "operator_name", &name).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn has_any_user_is_false_until_the_first_user_is_created() {
        let (_dir, pool) = test_pool().await;
        assert!(!has_any_user(&pool).await.unwrap());

        create(&pool, "Ayşe Yılmaz", "1234").await.unwrap();

        assert!(has_any_user(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn create_and_list_round_trip() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, "Ayşe Yılmaz", "1234").await.unwrap();

        assert_eq!(created.name, "Ayşe Yılmaz");
        assert!(created.is_active);

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed, vec![created]);
    }

    #[tokio::test]
    async fn create_rejects_a_duplicate_name_case_insensitively() {
        let (_dir, pool) = test_pool().await;
        create(&pool, "Ayşe Yılmaz", "1234").await.unwrap();

        let err = create(&pool, "AYŞE yılmaz", "5678").await.unwrap_err();

        assert!(matches!(err, AppError::Validation(_)));
        assert_eq!(list(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_rejects_a_blank_name() {
        let (_dir, pool) = test_pool().await;
        let err = create(&pool, "   ", "1234").await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn create_rejects_malformed_pins() {
        let (_dir, pool) = test_pool().await;
        for bad_pin in ["123", "1234567", "12a4", ""] {
            let err = create(&pool, "Test Kullanıcı", bad_pin).await.unwrap_err();
            assert!(matches!(err, AppError::Validation(_)), "pin: {bad_pin:?}");
        }
    }

    /// Brief'in vazgeçilmez testi: özet sütunu düz PIN'i hiçbir biçimde içermemeli.
    #[tokio::test]
    async fn pin_hash_column_never_contains_the_plain_pin() {
        let (_dir, pool) = test_pool().await;
        create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();

        let pin_hash: String = sqlx::query_scalar("SELECT pin_hash FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert!(!pin_hash.contains("4321"));
        assert!(pin_hash.starts_with("$argon2"));
    }

    #[tokio::test]
    async fn login_with_the_correct_pin_returns_true_and_sets_operator_name() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();

        let ok = login(&pool, user.id, "4321").await.unwrap();

        assert!(ok);
        assert_eq!(
            settings::get(&pool, "operator_name").await.unwrap().as_deref(),
            Some("Ayşe Yılmaz")
        );
    }

    #[tokio::test]
    async fn login_with_the_wrong_pin_returns_false_and_leaves_operator_name_untouched() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();
        settings::set(&pool, "operator_name", "önceki-deger").await.unwrap();

        let ok = login(&pool, user.id, "9999").await.unwrap();

        assert!(!ok);
        assert_eq!(
            settings::get(&pool, "operator_name").await.unwrap().as_deref(),
            Some("önceki-deger")
        );
    }

    #[tokio::test]
    async fn set_pin_replaces_the_old_pin() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();

        set_pin(&pool, user.id, "9999").await.unwrap();

        assert!(!login(&pool, user.id, "4321").await.unwrap());
        assert!(login(&pool, user.id, "9999").await.unwrap());
    }

    #[tokio::test]
    async fn rename_updates_the_listed_name() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();

        rename(&pool, user.id, "Ayşe Demir").await.unwrap();

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed[0].name, "Ayşe Demir");
    }

    /// Teşhis edilen kusur: oturum açmış kullanıcı kendi adını değiştirince
    /// `operator_name` (tarihçedeki `change_set.actor`ın kaynağı) da yeni
    /// adı yansıtmalı — yoksa çıkış yapana kadar hiçbir kullanıcıyla
    /// eşleşmeyen eski adla yazılmaya devam eder.
    #[tokio::test]
    async fn rename_updates_operator_name_when_the_logged_in_user_renames_itself() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();
        assert!(login(&pool, user.id, "4321").await.unwrap());

        rename(&pool, user.id, "Ayşe Demir").await.unwrap();

        assert_eq!(
            settings::get(&pool, "operator_name").await.unwrap().as_deref(),
            Some("Ayşe Demir")
        );
    }

    /// Oturumdaki kullanıcı BAŞKA biri iken bir kullanıcı yeniden
    /// adlandırılırsa `operator_name` dokunulmadan kalmalı — brief
    /// sözleşmesi: yalnız eski ad `operator_name`e eşitse güncellenir.
    #[tokio::test]
    async fn rename_leaves_operator_name_untouched_when_a_different_user_renames() {
        let (_dir, pool) = test_pool().await;
        let logged_in = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();
        let other = create(&pool, "Fatma Kaya", "1111").await.unwrap();
        assert!(login(&pool, logged_in.id, "4321").await.unwrap());

        rename(&pool, other.id, "Fatma Demir").await.unwrap();

        assert_eq!(
            settings::get(&pool, "operator_name").await.unwrap().as_deref(),
            Some("Ayşe Yılmaz")
        );
    }

    /// Çakışan adla rename reddedilir; reddedilen bir işlem `operator_name`e
    /// dokunmamalı — yarı tamamlanmış bir yazma olmadığını doğrular.
    #[tokio::test]
    async fn rename_with_a_conflicting_name_is_rejected_and_leaves_operator_name_untouched() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();
        create(&pool, "Fatma Kaya", "1111").await.unwrap();
        assert!(login(&pool, user.id, "4321").await.unwrap());

        let err = rename(&pool, user.id, "fatma  kaya").await.unwrap_err();

        assert!(matches!(err, AppError::Validation(_)));
        assert_eq!(
            settings::get(&pool, "operator_name").await.unwrap().as_deref(),
            Some("Ayşe Yılmaz")
        );
    }

    /// İkinci bir kullanıcı gerekli: tek kullanıcı varken pasifleştirme artık
    /// "son etkin kullanıcı" kuralıyla reddedilir (bkz.
    /// `set_active_refuses_to_deactivate_the_last_active_user`); bu test
    /// yalnız satırın KALDIRILMADIĞINI doğruluyor, o kuralı değil.
    #[tokio::test]
    async fn set_active_false_keeps_the_user_listed_but_marks_it_inactive() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();
        create(&pool, "Fatma Kaya", "1111").await.unwrap();

        set_active(&pool, user.id, false).await.unwrap();

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed.len(), 2, "yumuşak silme satırı KALDIRMAMALI");
        let updated = listed.iter().find(|u| u.id == user.id).unwrap();
        assert!(!updated.is_active);
    }

    /// Koordinatörün istediği kural: giriş ekranı yalnız aktifleri
    /// listeleyecek ama kimlik elle gönderilirse pasif bir kullanıcı PIN
    /// doğru olsa bile giremez — kural arayüzün filtresine bırakılmaz.
    #[tokio::test]
    async fn login_rejects_a_deactivated_user() {
        let (_dir, pool) = test_pool().await;
        let user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();
        // Guard'ı tetiklememek için ikinci bir aktif kullanıcı gerekiyor.
        create(&pool, "Fatma Kaya", "1111").await.unwrap();
        assert!(login(&pool, user.id, "4321").await.unwrap(), "pasifleşmeden önce girebilmeli");
        settings::set(&pool, "operator_name", "önceki-deger").await.unwrap();

        set_active(&pool, user.id, false).await.unwrap();
        let ok = login(&pool, user.id, "4321").await.unwrap();

        assert!(!ok, "pasif kullanıcı doğru PIN'le bile giremez");
        assert_eq!(
            settings::get(&pool, "operator_name").await.unwrap().as_deref(),
            Some("önceki-deger")
        );
    }

    /// Son etkin kullanıcı pasifleştirilemez: giriş ekranı yalnız aktifleri
    /// listeliyor, aktif sayısı sıfıra düşerse uygulama veritabanı elle
    /// düzeltilmeden açılamaz hâle gelir.
    #[tokio::test]
    async fn set_active_refuses_to_deactivate_the_last_active_user() {
        let (_dir, pool) = test_pool().await;
        let only_user = create(&pool, "Ayşe Yılmaz", "4321").await.unwrap();

        let err = set_active(&pool, only_user.id, false).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert!(list(&pool).await.unwrap()[0].is_active, "hâlâ aktif kalmalı");

        let second_user = create(&pool, "Fatma Kaya", "1111").await.unwrap();
        set_active(&pool, only_user.id, false)
            .await
            .expect("ikinci bir aktif kullanıcı varken pasifleştirme geçmeli");

        let listed = list(&pool).await.unwrap();
        let first = listed.iter().find(|u| u.id == only_user.id).unwrap();
        let second = listed.iter().find(|u| u.id == second_user.id).unwrap();
        assert!(!first.is_active);
        assert!(second.is_active);
    }
}
