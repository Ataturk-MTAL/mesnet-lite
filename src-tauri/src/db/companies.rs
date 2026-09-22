use crate::domain::address::parse_district;
use crate::domain::models::{Company, NewCompany};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

const SELECT_COLUMNS: &str = "id, name, contact_first_name, contact_last_name, phone, email, \
     address_text, latitude, longitude, geocode_status, one_way_distance_km, district, notes, \
     created_at, updated_at";

/// İlçeyi belirler: kullanıcı elle girdiyse (boş olmayan bir değer
/// gönderdiyse) o değer KORUNUR ve EZİLMEZ; boşsa adresten türetilir.
/// Adresten de çıkarılamazsa boş kalır — bu bir hata değildir, yalnızca
/// ilçe bazlı gruplamada işletme "ilçesiz" görünür.
fn resolve_district(explicit_district: &str, address_text: &str) -> String {
    let trimmed = explicit_district.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    parse_district(address_text).unwrap_or_default()
}

/// Mükerrer tespiti için işletme adını normalize eder: Unicode-doğru küçük harf
/// ve ardışık boşlukları teke indirme. SQLite'ın UPPER()/LOWER() fonksiyonları
/// yalnızca ASCII dönüştürdüğü için bu iş SQL'de değil burada yapılır.
pub fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn not_found(id: i64) -> AppError {
    AppError::NotFound(format!("İşletme bulunamadı: {id}"))
}

/// Yalnızca AKTİF işletmeler. Seçici/yönetim ekranları (İşletmeler listesi,
/// birleştirme hedefi, saat/atama havuzu) bunu kullanır: pasif bir işletme
/// "Sil" denip kaldırılmış GİBİ görünmeli, yeniden atanabilir bir seçenek
/// olarak çıkmamalı (spec §5.4). Dönem ortasında pasifleşen bir işletmeyi
/// GİZLEMEMESİ gereken çağıranlar (rapor/dışa aktarım/tutanak/birleştirme
/// mükerrer araması) bunun yerine `list_all`ı kullanır.
pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Company>> {
    let sql =
        format!("SELECT {SELECT_COLUMNS} FROM companies WHERE is_active = 1 ORDER BY name COLLATE NOCASE");
    Ok(sqlx::query_as::<_, Company>(&sql).fetch_all(pool).await?)
}

/// `list`in süzülmemiş hâli: pasif işletmeler DE döner. Bir rapor/dışa
/// aktarım dönem ortasında pasifleşen bir işletmeyi göstermezse tutanak
/// bozulur; bu yüzden geçmişe bakan her çağıran bunu kullanmalı.
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Company>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM companies ORDER BY name COLLATE NOCASE");
    Ok(sqlx::query_as::<_, Company>(&sql).fetch_all(pool).await?)
}

/// `get` ASLA süzmez: geçmişteki bir kaydın adını çözmek için pasif bir
/// işletmeye de erişilebilmesi gerekir (ör. tarihçe ekranı, birleştirme
/// sonrası kaynak kaydın adı).
pub async fn get(pool: &SqlitePool, id: i64) -> AppResult<Company> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM companies WHERE id = ?1");
    Ok(sqlx::query_as::<_, Company>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

/// `is_active` bayrağı `Company`'ye taşınmaz (ekranların çoğu zaten yalnız
/// aktif işletmeleri gösterir); işletme birleştirme gibi pasiflik denetimi
/// GEREKEN az sayıdaki çağıran bunu ayrı okur (`services::company_merge`).
pub async fn is_active(pool: &SqlitePool, id: i64) -> AppResult<bool> {
    let value: Option<i64> = sqlx::query_scalar("SELECT is_active FROM companies WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    value.map(|v| v != 0).ok_or_else(|| not_found(id))
}

pub async fn create(pool: &SqlitePool, input: &NewCompany) -> AppResult<Company> {
    let now = now_iso();
    // Konum verilmişse kayıt 'manual', verilmemişse 'pending' başlar.
    let status = if input.latitude.is_some() && input.longitude.is_some() {
        "manual"
    } else {
        "pending"
    };

    let district = resolve_district(&input.district, &input.address_text);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO companies
            (name, contact_first_name, contact_last_name, phone, email, address_text,
             latitude, longitude, geocode_status, one_way_distance_km, district, notes,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.contact_first_name)
    .bind(&input.contact_last_name)
    .bind(&input.phone)
    .bind(&input.email)
    .bind(&input.address_text)
    .bind(input.latitude)
    .bind(input.longitude)
    .bind(status)
    .bind(input.one_way_distance_km)
    .bind(district)
    .bind(&input.notes)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;

    get(pool, id).await
}

/// `change_service::execute_in` (R4) yerinde oluşturma adımı içindir
/// (spec §5 adım 2, "Yerinde oluşturma"): `decide`'dan ÖNCE, AYNI
/// transaction'daki bağlantı üzerinden çağrılır — havuzdan yeni bir
/// bağlantı ALINMAZ (plan "Genel Kısıtlar": transaction içinde havuz
/// kullanmak yasaktır).
pub async fn create_in(conn: &mut SqliteConnection, input: &NewCompany) -> AppResult<Company> {
    let now = now_iso();
    let status = if input.latitude.is_some() && input.longitude.is_some() { "manual" } else { "pending" };
    let district = resolve_district(&input.district, &input.address_text);
    let sql = format!(
        "INSERT INTO companies
            (name, contact_first_name, contact_last_name, phone, email, address_text,
             latitude, longitude, geocode_status, one_way_distance_km, district, notes,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         RETURNING {SELECT_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, Company>(&sql)
        .bind(&input.name)
        .bind(&input.contact_first_name)
        .bind(&input.contact_last_name)
        .bind(&input.phone)
        .bind(&input.email)
        .bind(&input.address_text)
        .bind(input.latitude)
        .bind(input.longitude)
        .bind(status)
        .bind(input.one_way_distance_km)
        .bind(district)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .fetch_one(&mut *conn)
        .await?)
}

/// Yumuşak silme: geçmişi olan bir işletme veritabanından SİLİNEMEZ
/// (spec §5.4), yerine pasif yapılır (`companies.is_active`, `0006`).
pub async fn set_active_in(conn: &mut SqliteConnection, id: i64, is_active: bool) -> AppResult<()> {
    let affected = sqlx::query("UPDATE companies SET is_active = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(i64::from(is_active))
        .bind(now_iso())
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

pub async fn update(pool: &SqlitePool, id: i64, input: &NewCompany) -> AppResult<Company> {
    // created_at korunur; yalnızca updated_at tazelenir.
    // Konum burada değişmez; onun için set_location kullanılır.
    // İlçe elle verilmemişse (boşsa) GÜNCEL adresten yeniden türetilir; bu
    // yüzden adres değişip ilçe boş bırakılırsa ilçe de değişmiş olur.
    let district = resolve_district(&input.district, &input.address_text);
    let affected = sqlx::query(
        "UPDATE companies SET
            name = ?1, contact_first_name = ?2, contact_last_name = ?3, phone = ?4,
            email = ?5, address_text = ?6, one_way_distance_km = ?7, district = ?8,
            notes = ?9, updated_at = ?10
         WHERE id = ?11",
    )
    .bind(&input.name)
    .bind(&input.contact_first_name)
    .bind(&input.contact_last_name)
    .bind(&input.phone)
    .bind(&input.email)
    .bind(&input.address_text)
    .bind(input.one_way_distance_km)
    .bind(district)
    .bind(&input.notes)
    .bind(now_iso())
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    get(pool, id).await
}

/// `update`in aynı transaction'daki bağlantı üzerinden çalışan hâli.
/// `services::import_apply` (CSV içe aktarımı) TÜM okuma ve yazmayı tek
/// `BEGIN IMMEDIATE` transaction'ında yapar (`create_in` üstündeki yorumla
/// aynı gerekçe): transaction açıkken havuzdan yazmak "database is locked"
/// hatası verir.
pub async fn update_in(conn: &mut SqliteConnection, id: i64, input: &NewCompany) -> AppResult<Company> {
    let district = resolve_district(&input.district, &input.address_text);
    let sql = format!(
        "UPDATE companies SET
            name = ?1, contact_first_name = ?2, contact_last_name = ?3, phone = ?4,
            email = ?5, address_text = ?6, one_way_distance_km = ?7, district = ?8,
            notes = ?9, updated_at = ?10
         WHERE id = ?11
         RETURNING {SELECT_COLUMNS}"
    );
    sqlx::query_as::<_, Company>(&sql)
        .bind(&input.name)
        .bind(&input.contact_first_name)
        .bind(&input.contact_last_name)
        .bind(&input.phone)
        .bind(&input.email)
        .bind(&input.address_text)
        .bind(input.one_way_distance_km)
        .bind(district)
        .bind(&input.notes)
        .bind(now_iso())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| not_found(id))
}

/// Silmenin ne yaptığını arayüze söyler: geçmişi olan bir işletme silinmez,
/// pasife alınır (spec §5.4). Kullanıcı "sildim" sanıp veriyi kaybettiğini
/// düşünmesin diye ikisi ayırt edilir.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyRemoval {
    pub soft_deleted: bool,
}

/// İşletmenin veritabanında iz bırakıp bırakmadığını denetler (spec §5.4):
/// iz varsa `remove` sert silme YAPAMAZ, pasife almalıdır. Şemadaki HER
/// `company_id` taşıyan canlı tablo denetlenir:
/// - `student_placements`, `company_hour_periods`, `coordination_periods`
///   (tarihçe projeksiyonları, migration 0006 — BİLEREK FK'siz: bir satır
///   sahibinden uzun yaşayabilmeli, spec §4.1) — sert silme bunlarda
///   sarkan `company_id` bırakırdı, teşhis edilen asıl zarar buydu.
/// - `company_term_hours`, `assignments` (migration 0004'ün event-sourced
///   OLMAYAN, hâlâ canlı okunan/yazılan eski çifti — bkz. db/company_hours.rs,
///   db/assignments.rs). FK'leri `ON DELETE CASCADE` olsa da sert silme bu
///   satırları SESSİZCE yok ederdi; kullanıcı veriyi kaybettiğini fark etmezdi.
/// - `change_events` (yalnız işletmenin özne olduğu `company_hours`/
///   `coordination` akışları; `subject_id` migration 0006'da BİLEREK FK'siz).
///
/// `students.company_id` BİLEREK DIŞARIDA bırakılır: migration 0006'dan beri
/// donmuş, hiçbir yol onu okumaz/yazmaz (bkz. db/students.rs SELECT_COLUMNS
/// yorumu, "artık okunmuyor"). Denetime dahil edilseydi, birleştirmeyle
/// zaten pasifleşmiş HER işletme (`company_merge` kaynak kaydı gibi) bu
/// donuk sütun yüzünden anlamsızca "geçmişi var" sayılırdı.
async fn has_history(conn: &mut SqliteConnection, id: i64) -> AppResult<bool> {
    let found: i64 = sqlx::query_scalar(
        "SELECT
             EXISTS(SELECT 1 FROM student_placements WHERE company_id = ?1)
          OR EXISTS(SELECT 1 FROM company_hour_periods WHERE company_id = ?2)
          OR EXISTS(SELECT 1 FROM coordination_periods WHERE company_id = ?3)
          OR EXISTS(SELECT 1 FROM company_term_hours WHERE company_id = ?4)
          OR EXISTS(SELECT 1 FROM assignments WHERE company_id = ?5)
          OR EXISTS(
                 SELECT 1 FROM change_events
                 WHERE subject_id = ?6 AND stream IN ('company_hours', 'coordination')
             )",
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(found != 0)
}

/// Geçmişi olan bir işletme yumuşak silinir (`is_active = 0`), olmayan
/// gerçekten silinir (spec §5.4). Var olma + geçmiş denetimi + yazma TEK
/// transaction'da yapılır: aradaki bir yarışta (ör. tam bu sırada bir
/// yerleştirme yazılırsa) sert silmenin denetimi atlayıp iz bırakmadan
/// geçmesini önler.
pub async fn remove(pool: &SqlitePool, id: i64) -> AppResult<CompanyRemoval> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM companies WHERE id = ?1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(not_found(id));
    }

    if has_history(&mut tx, id).await? {
        set_active_in(&mut tx, id, false).await?;
        tx.commit().await?;
        return Ok(CompanyRemoval { soft_deleted: true });
    }

    sqlx::query("DELETE FROM companies WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(CompanyRemoval { soft_deleted: false })
}

/// Konumu yazar ve durumu günceller. `status` coğrafi kodlamadan geliyorsa
/// 'resolved', haritadan elle işaretlendiyse 'manual' olur.
pub async fn set_location(
    pool: &SqlitePool,
    id: i64,
    latitude: f64,
    longitude: f64,
    status: &str,
) -> AppResult<Company> {
    let affected = sqlx::query(
        "UPDATE companies SET latitude = ?1, longitude = ?2, geocode_status = ?3, updated_at = ?4
         WHERE id = ?5",
    )
    .bind(latitude)
    .bind(longitude)
    .bind(status)
    .bind(now_iso())
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(not_found(id));
    }
    get(pool, id).await
}

/// Coğrafi kodlama başarısızlığını kaydeder. Hata değil, kayda geçen bir durumdur.
pub async fn mark_geocode_failed(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("UPDATE companies SET geocode_status = 'failed', updated_at = ?1 WHERE id = ?2")
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Normalize edilmiş ada göre mevcut kaydı arar (CSV içe aktarmada mükerrer tespiti).
/// `list_all` kullanır, `list` DEĞİL: pasif bir işletmeyle aynı adı taşıyan
/// bir kayıt yeniden içe aktarılırsa kullanıcı mevcut (pasif) kaydı görüp
/// birleştirme/güncelleme kararı verebilmeli — süzülmüş liste bu eşleşmeyi
/// gizleyip mükerrer bir aktif kayıt açılmasına yol açardı.
/// Karşılaştırma bellekte yapılır: kayıt sayısı birkaç yüzü geçmez ve SQL tarafında
/// Unicode-doğru küçük harf dönüşümü yoktur.
pub async fn find_by_normalized_name(pool: &SqlitePool, name: &str) -> AppResult<Option<Company>> {
    let target = normalize_name(name);
    let rows = list_all(pool).await?;
    Ok(rows.into_iter().find(|c| normalize_name(&c.name) == target))
}

/// `find_by_normalized_name`in aynı transaction'daki bağlantı üzerinden
/// çalışan hâli — `services::import_apply` bunu kullanır (`update_in`
/// üstündeki yorumla aynı gerekçe).
pub async fn find_by_normalized_name_in(conn: &mut SqliteConnection, name: &str) -> AppResult<Option<Company>> {
    let target = normalize_name(name);
    let sql = format!("SELECT {SELECT_COLUMNS} FROM companies ORDER BY name COLLATE NOCASE");
    let rows: Vec<Company> = sqlx::query_as(&sql).fetch_all(&mut *conn).await?;
    Ok(rows.into_iter().find(|c| normalize_name(&c.name) == target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::company_hours::{self, HoursInput};
    use crate::db::{init_pool, teachers};
    use crate::domain::models::NewTeacher;

    /// Tüm geçmiş-tablosu testlerinin paylaştığı dönem; `terms` ve o dönem
    /// için bir `opening` `change_sets` satırı migration 0006'nın göç
    /// tohumunda bu değer için zaten açılır (bkz. 0001'in
    /// `settings.active_term` varsayılanı).
    const TERM: &str = "2026-2027/1";

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    /// `student_placements`/`company_hour_periods`/`coordination_periods`nin
    /// `source_event_id` FK'sini tatmin etmek için TEK bir `change_events`
    /// satırı açar. Olayın içeriği `has_history` denetiminde önemli değildir
    /// — yalnız satırın VAR OLMASI önemlidir.
    async fn seed_change_event(conn: &mut SqliteConnection, stream: &str, subject_id: i64) -> i64 {
        let change_set_id: i64 = sqlx::query_scalar(
            "INSERT INTO change_sets (term, kind, effective_date, reason, actor, recorded_at, impact_json)
             VALUES (?1, 'test', '2026-09-01', '', '', '2026-09-01T00:00:00Z', '{}') RETURNING id",
        )
        .bind(TERM)
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        sqlx::query_scalar(
            "INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload)
             VALUES (?1, ?2, ?3, ?4, 'test_kind', 1, '2026-09-01', '{}') RETURNING id",
        )
        .bind(change_set_id)
        .bind(stream)
        .bind(subject_id)
        .bind(TERM)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    }

    async fn a_teacher(pool: &SqlitePool) -> i64 {
        teachers::create(
            pool,
            &NewTeacher {
                first_name: "Test".into(),
                last_name: "Öğretmen".into(),
                registry_no: String::new(),
                field: "Elektrik-Elektronik Teknolojisi".into(),
                branches: vec![],
                employment_type: "tenured".into(),
                base_hours: 20,
                max_extra_hours: 24,
                other_extra_hours: 0,
                chief_type: "none".into(),
                is_active: true,
            },
        )
        .await
        .unwrap()
        .id
    }

    fn sample_input(name: &str) -> NewCompany {
        NewCompany {
            name: name.into(),
            contact_first_name: "Test".into(),
            contact_last_name: "Yetkili".into(),
            phone: "(500) 000-0000".into(),
            email: String::new(),
            address_text: "Test Mahallesi, Test Sokak No:1, Mersin".into(),
            latitude: None,
            longitude: None,
            one_way_distance_km: Some(6.8),
            district: String::new(),
            notes: String::new(),
        }
    }

    #[tokio::test]
    async fn create_then_get_returns_same_record() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();
        let fetched = get(&pool, created.id).await.unwrap();

        assert_eq!(fetched.name, "Test İşletme A");
        assert_eq!(fetched.one_way_distance_km, Some(6.8));
        // Konumsuz oluşturulan kayıt 'pending' başlar.
        assert_eq!(fetched.geocode_status, "pending");
    }

    #[tokio::test]
    async fn list_returns_records_sorted_by_name() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample_input("Zeta Teknik")).await.unwrap();
        create(&pool, &sample_input("Alfa Elektronik")).await.unwrap();

        let rows = list(&pool).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Alfa Elektronik");
    }

    /// `list` pasif işletmeyi GİZLER (yönetim/seçici ekranlar için); `list_all`
    /// ve `get` gizlemez — `get` geçmişteki bir kaydın adını çözmek için,
    /// `list_all` rapor/dışa aktarım için pasifi de göstermek zorunda.
    #[tokio::test]
    async fn list_hides_inactive_companies_but_list_all_and_get_do_not() {
        let (_dir, pool) = test_pool().await;
        let active = create(&pool, &sample_input("Aktif A.Ş.")).await.unwrap();
        let inactive = create(&pool, &sample_input("Pasif A.Ş.")).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        set_active_in(&mut conn, inactive.id, false).await.unwrap();
        drop(conn);

        let visible = list(&pool).await.unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, active.id);

        let all = list_all(&pool).await.unwrap();
        assert_eq!(all.len(), 2);

        assert!(get(&pool, inactive.id).await.is_ok(), "get pasif kaydı da döndürmeli");
    }

    #[tokio::test]
    async fn update_changes_fields_and_keeps_created_at() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();

        let mut input = sample_input("Test İşletme A - Yeni");
        input.one_way_distance_km = Some(12.0);
        let updated = update(&pool, created.id, &input).await.unwrap();

        assert_eq!(updated.name, "Test İşletme A - Yeni");
        assert_eq!(updated.one_way_distance_km, Some(12.0));
        assert_eq!(updated.created_at, created.created_at);
    }

    #[tokio::test]
    async fn remove_hard_deletes_a_company_with_no_history() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();

        let result = remove(&pool, created.id).await.unwrap();

        assert!(!result.soft_deleted, "geçmişi olmayan işletme gerçekten silinmeli");
        let err = get(&pool, created.id).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn remove_missing_company_returns_not_found() {
        let (_dir, pool) = test_pool().await;
        let err = remove(&pool, 999).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    /// Geçmişi olan bir işletme SİLİNMEZ, pasife alınır (spec §5.4). Her
    /// history tablosu için ayrı senaryo: birini atlarsak sert silme o
    /// tabloda sarkan `company_id` üretmeye devam eder.
    #[tokio::test]
    async fn remove_soft_deletes_a_company_with_an_open_student_placement() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Tarihçeli A.Ş.")).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let event_id = seed_change_event(&mut conn, "placement", created.id).await;
        sqlx::query(
            "INSERT INTO student_placements (student_id, term, company_id, valid_from, valid_to, source_event_id)
             VALUES (999, ?1, ?2, '2026-09-01', NULL, ?3)",
        )
        .bind(TERM)
        .bind(created.id)
        .bind(event_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let result = remove(&pool, created.id).await.unwrap();

        assert!(result.soft_deleted, "açık yerleştirmesi olan işletme silinmemeli");
        assert!(get(&pool, created.id).await.is_ok(), "kayıt veritabanında kalmalı");
        assert!(!is_active(&pool, created.id).await.unwrap());

        // Asıl teşhis edilen zarar: sarkan bir company_id ÜRETİLMEMELİ.
        let still_points_at_the_company: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM student_placements WHERE company_id = ?1",
        )
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(still_points_at_the_company, 1, "yerleştirme satırı hâlâ var olan bir işletmeye işaret etmeli");
    }

    #[tokio::test]
    async fn remove_soft_deletes_a_company_with_a_company_hour_period() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Saat Geçmişli A.Ş.")).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let event_id = seed_change_event(&mut conn, "company_hours", created.id).await;
        sqlx::query(
            "INSERT INTO company_hour_periods
                (company_id, term, valid_from, valid_to, awarded_hours, max_hours_snapshot, source_event_id)
             VALUES (?1, ?2, '2026-09-01', NULL, 6, 8, ?3)",
        )
        .bind(created.id)
        .bind(TERM)
        .bind(event_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let result = remove(&pool, created.id).await.unwrap();

        assert!(result.soft_deleted, "saat takdiri geçmişi olan işletme silinmemeli");
    }

    #[tokio::test]
    async fn remove_soft_deletes_a_company_with_a_coordination_period() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Koordinasyon Geçmişli A.Ş.")).await.unwrap();
        let teacher_id = a_teacher(&pool).await;

        let mut conn = pool.acquire().await.unwrap();
        let event_id = seed_change_event(&mut conn, "coordination", created.id).await;
        sqlx::query(
            "INSERT INTO coordination_periods
                (company_id, term, valid_from, valid_to, teacher_id, visit_day, visit_hour, source_event_id)
             VALUES (?1, ?2, '2026-09-01', NULL, ?3, 1, 3, ?4)",
        )
        .bind(created.id)
        .bind(TERM)
        .bind(teacher_id)
        .bind(event_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let result = remove(&pool, created.id).await.unwrap();

        assert!(result.soft_deleted, "koordinasyon geçmişi olan işletme silinmemeli");
    }

    #[tokio::test]
    async fn remove_soft_deletes_a_company_with_company_term_hours() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Takdir Geçmişli A.Ş.")).await.unwrap();
        company_hours::upsert(
            &pool,
            TERM,
            &HoursInput {
                company_id: created.id,
                max_hours_snapshot: 8,
                awarded_hours: 6,
                is_honorary: false,
                is_locked: false,
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        let result = remove(&pool, created.id).await.unwrap();

        assert!(result.soft_deleted, "company_term_hours satırı olan işletme silinmemeli");
        // Sert silinseydi FK ON DELETE CASCADE bu satırı SESSİZCE yok ederdi.
        assert!(company_hours::get(&pool, created.id, TERM).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn remove_soft_deletes_a_company_with_an_assignment() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Atama Geçmişli A.Ş.")).await.unwrap();
        let teacher_id = a_teacher(&pool).await;
        sqlx::query(
            "INSERT INTO assignments (teacher_id, company_id, term, visit_day, visit_hour, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, 3, '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        )
        .bind(teacher_id)
        .bind(created.id)
        .bind(TERM)
        .execute(&pool)
        .await
        .unwrap();

        let result = remove(&pool, created.id).await.unwrap();

        assert!(result.soft_deleted, "atama geçmişi olan işletme silinmemeli");
    }

    #[tokio::test]
    async fn remove_soft_deletes_a_company_with_a_change_event_but_no_projection_row() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Yalnız Olay Geçmişli A.Ş.")).await.unwrap();

        // Yalnızca change_events'te iz var; hiçbir projeksiyon satırı yok
        // (ör. olay katlanıp `valid_to` kapanmış, açık satır kalmamış olabilir).
        let mut conn = pool.acquire().await.unwrap();
        seed_change_event(&mut conn, "company_hours", created.id).await;
        drop(conn);

        let result = remove(&pool, created.id).await.unwrap();

        assert!(result.soft_deleted, "change_events'teki geçmiş de silmeyi engellemeli");
    }

    #[tokio::test]
    async fn update_missing_record_returns_not_found() {
        let (_dir, pool) = test_pool().await;
        let err = update(&pool, 999, &sample_input("Yok")).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn set_location_marks_status_manual() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();

        let located = set_location(&pool, created.id, 36.8, 34.6, "manual")
            .await
            .unwrap();

        assert_eq!(located.latitude, Some(36.8));
        assert_eq!(located.longitude, Some(34.6));
        assert_eq!(located.geocode_status, "manual");
    }

    #[tokio::test]
    async fn create_in_writes_through_the_given_connection() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let created = create_in(&mut conn, &sample_input("Bağlantı Testi A.Ş.")).await.unwrap();
        let fetched = get(&pool, created.id).await.unwrap();
        assert_eq!(fetched.name, "Bağlantı Testi A.Ş.");
    }

    #[tokio::test]
    async fn update_in_writes_through_the_given_connection() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Bağlantı Testi A.Ş.")).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let mut input = sample_input("Bağlantı Testi A.Ş. — Güncel");
        input.one_way_distance_km = Some(9.5);
        let updated = update_in(&mut conn, created.id, &input).await.unwrap();

        assert_eq!(updated.name, "Bağlantı Testi A.Ş. — Güncel");
        assert_eq!(updated.one_way_distance_km, Some(9.5));
        assert_eq!(updated.created_at, created.created_at, "created_at korunmalı");
    }

    #[tokio::test]
    async fn update_in_missing_record_returns_not_found() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let err = update_in(&mut conn, 999, &sample_input("Yok")).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn find_by_normalized_name_in_matches_regardless_of_case_and_spacing() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample_input("MEKA OTOMASYON")).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let found = find_by_normalized_name_in(&mut conn, "  meka   otomasyon ").await.unwrap();
        assert!(found.is_some());

        let missing = find_by_normalized_name_in(&mut conn, "baska isletme").await.unwrap();
        assert!(missing.is_none());
    }

    /// `create_in` (transaction içi yerinde oluşturma) de `create` ile AYNI
    /// ilçe türetme kuralını uygulamalı; iki yol arasında davranış farkı
    /// olursa yalnız birinde test edilen kural fiilen uygulanmamış olur.
    #[tokio::test]
    async fn create_in_derives_district_from_address_when_left_blank() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let mut input = sample_input("Bağlantı Testi A.Ş.");
        input.address_text = "33130 Akdeniz/Mersin".into();

        let created = create_in(&mut conn, &input).await.unwrap();

        assert_eq!(created.district, "Akdeniz");
    }

    #[tokio::test]
    async fn set_active_in_toggles_the_flag() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Pasifleşecek A.Ş.")).await.unwrap();

        let active_before: i64 = sqlx::query_scalar("SELECT is_active FROM companies WHERE id = ?1")
            .bind(created.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(active_before, 1, "yeni işletme aktif başlamalı");

        let mut conn = pool.acquire().await.unwrap();
        set_active_in(&mut conn, created.id, false).await.unwrap();

        let active_after: i64 = sqlx::query_scalar("SELECT is_active FROM companies WHERE id = ?1")
            .bind(created.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(active_after, 0);
    }

    #[tokio::test]
    async fn is_active_reflects_the_flag_and_missing_id_is_not_found() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();
        assert!(is_active(&pool, created.id).await.unwrap(), "yeni işletme aktif başlamalı");

        let mut conn = pool.acquire().await.unwrap();
        set_active_in(&mut conn, created.id, false).await.unwrap();
        assert!(!is_active(&pool, created.id).await.unwrap());

        let err = is_active(&pool, 999).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn set_active_in_missing_company_returns_not_found() {
        let (_dir, pool) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let err = set_active_in(&mut conn, 999, false).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn mark_geocode_failed_sets_failed_status() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();

        mark_geocode_failed(&pool, created.id).await.unwrap();

        assert_eq!(
            get(&pool, created.id).await.unwrap().geocode_status,
            "failed"
        );
    }

    /// İlçe boş bırakılırsa adresten türetilmeli.
    #[tokio::test]
    async fn create_derives_district_from_address_when_left_blank() {
        let (_dir, pool) = test_pool().await;
        let mut input = sample_input("Test İşletme A");
        input.address_text = "Mega Center, Çilek, 63143 sokak D:8. Blok No:4, 33020 Akdeniz/Mersin, Türkiye".into();

        let created = create(&pool, &input).await.unwrap();

        assert_eq!(created.district, "Akdeniz");
    }

    /// Kullanıcı ilçeyi elle girdiyse EZİLMEMELİ; adresten türetilen değer
    /// farklı olsa bile elle girilen korunur.
    #[tokio::test]
    async fn create_keeps_the_explicit_district_instead_of_deriving_it() {
        let (_dir, pool) = test_pool().await;
        let mut input = sample_input("Test İşletme A");
        input.address_text = "33020 Akdeniz/Mersin, Türkiye".into();
        input.district = "Toroslar".into();

        let created = create(&pool, &input).await.unwrap();

        assert_eq!(created.district, "Toroslar");
    }

    /// Adresten hiç ilçe çıkarılamazsa ilçe boş kalmalı; hata fırlatılmamalı.
    #[tokio::test]
    async fn create_leaves_district_blank_when_address_does_not_match() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();

        assert_eq!(created.district, "");
    }

    /// Güncellemede ilçe elle verilmemişse GÜNCEL adresten yeniden türetilir.
    #[tokio::test]
    async fn update_rederives_district_when_address_changes_and_district_is_blank() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();
        assert_eq!(created.district, "", "başlangıç adresinde posta kodu yok");

        let mut input = sample_input("Test İşletme A");
        input.address_text = "33130 Akdeniz/Mersin".into();
        let updated = update(&pool, created.id, &input).await.unwrap();

        assert_eq!(updated.district, "Akdeniz");
    }

    /// Güncellemede ilçe elle verilmişse (boş değilse) korunmalı; adres
    /// değişse bile ezilmez.
    #[tokio::test]
    async fn update_keeps_the_explicit_district_even_if_address_changes() {
        let (_dir, pool) = test_pool().await;
        let created = create(&pool, &sample_input("Test İşletme A")).await.unwrap();

        let mut input = sample_input("Test İşletme A");
        input.address_text = "33130 Akdeniz/Mersin".into();
        input.district = "Toroslar".into();
        let updated = update(&pool, created.id, &input).await.unwrap();

        assert_eq!(updated.district, "Toroslar");
    }

    /// Mükerrer tespiti Unicode-doğru olmalı; SQL tarafında yapılamaz.
    #[test]
    fn normalize_name_lowercases_and_collapses_spaces() {
        assert_eq!(
            normalize_name("  RITIMSAN   ELEKTRONIK  "),
            "ritimsan elektronik"
        );
        // Aynı ad farklı yazımlarda aynı anahtara inmeli.
        assert_eq!(
            normalize_name("MEKA OTOMASYON"),
            normalize_name("  meka   otomasyon ")
        );
    }

    #[tokio::test]
    async fn find_by_normalized_name_matches_regardless_of_case_and_spacing() {
        let (_dir, pool) = test_pool().await;
        create(&pool, &sample_input("MEKA OTOMASYON")).await.unwrap();

        let found = find_by_normalized_name(&pool, "  meka   otomasyon ")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "MEKA OTOMASYON");

        let missing = find_by_normalized_name(&pool, "baska isletme")
            .await
            .unwrap();
        assert!(missing.is_none());
    }
}
