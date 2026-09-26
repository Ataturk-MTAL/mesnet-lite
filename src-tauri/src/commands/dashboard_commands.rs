use crate::db::{assignments, companies, company_hours, settings, students, teachers, teaching_load, AppState};
use crate::domain::workload::{statutory_cap, teacher_capacity, InstitutionType};
use crate::error::AppResult;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

/// Genel Bakış ekranının sayaçları ve saat dengesi.
/// Tek çağrıda toplanır ki ekran ayrı ayrı istek atmasın.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    /// Sayıların ait olduğu eğitim-öğretim yılı.
    pub term: String,

    // Kayıt sayıları
    /// İşletmeler kalıcıdır; dönemden bağımsız toplam.
    pub company_count: i64,
    /// Yalnızca aktif dönemdeki öğrenciler.
    pub student_count: i64,
    pub teacher_count: i64,
    pub active_teacher_count: i64,

    // Saat dengesi — hepsi aktif dönem içindir
    /// Aktif öğretmenlerin koordinatörlük kapasiteleri toplamı (dağıtılabilir azami).
    pub total_capacity_hours: i64,
    /// Atamalarda takdir edilmiş toplam saat (dağıtılmış).
    pub assigned_hours: i64,
    /// Dağıtılabilir azamiden kalan. Aşım varsa negatif olur ve uyarı anlamına gelir.
    pub remaining_hours: i64,
    /// Kapasitesi dolmuş aktif öğretmen sayısı.
    pub teachers_at_capacity: i64,
    /// Kapasitesi aşılmış aktif öğretmen sayısı — mevzuat ihlalidir.
    pub teachers_over_capacity: i64,

    // Alan koordinatörlüğü ders yükü havuzu (OÖKY MADDE 88/2-ç, Norm Kadro
    // Yön. MADDE 6/4) — yukarıdaki öğretmen kapasitesi dengesinden AYRI bir
    // eksendir; kullanıcı ikisini yan yana görmek istedi.
    /// Havuzun TEK hesap noktası: `db::teaching_load::total_pool_hours`.
    pub pool_hours: i64,
    /// İşletmelere TAKDİR EDİLEN toplam saat (atanan DEĞİL — kullanıcı bunu
    /// açıkça seçti). Fahri satırlar 0 saat taşıdığı için katkı vermez.
    pub awarded_hours: i64,
    /// `poolHours - awardedHours`. Havuz takdirden küçükse (göçten kalma ya
    /// da havuz sonradan küçültülmüş veri) negatif olur.
    pub remaining_pool_hours: i64,

    // Eksik kayıtlar
    pub companies_without_location: i64,
    pub students_without_company: i64,
    pub companies_without_students: i64,
    /// Aktif dönemde öğrencisi olup koordinatörü atanmamış işletmeler.
    pub companies_without_assignment: i64,
}

async fn dashboard_stats(pool: &SqlitePool) -> AppResult<DashboardStats> {
    let all = settings::get_all(pool).await?;
    let term = all.get("active_term").cloned().unwrap_or_default();

    let institution_type = InstitutionType::parse(
        all.get("institution_type").map(String::as_str).unwrap_or("other"),
    );
    let is_metropolitan = all
        .get("is_metropolitan_district")
        .map(|v| v == "true")
        .unwrap_or(false);
    let cap = statutory_cap(institution_type, is_metropolitan);

    // `list` (süzülmüş): pano bir yönetim ekranıdır, pasif işletme sayılara katılmaz.
    let all_companies = companies::list(pool).await?;
    let all_students = students::list_by_term(pool, &term).await?;
    // Kapasite `teacher_load_periods` PROJEKSİYONUNDAN okunur (spec R5c);
    // eski `teachers.chief_type`/yük sütunları artık burada okunmaz.
    let as_of = teaching_load::current_as_of(pool, &term).await?;
    let all_teachers = teachers::list_with_load_as_of(pool, &term, as_of).await?;
    let student_counts = students::count_by_company(pool, &term).await?;
    let awarded_by_teacher = assignments::awarded_hours_by_teacher(pool, &term).await?;
    let assigned_companies: std::collections::HashSet<i64> =
        assignments::assigned_company_ids(pool, &term).await?.into_iter().collect();

    let companies_with_students: std::collections::HashSet<i64> =
        student_counts.iter().map(|(id, _)| *id).collect();

    // Saat dengesi yalnızca aktif öğretmenler üzerinden hesaplanır;
    // pasif öğretmene atama yapılmaz.
    let mut total_capacity_hours = 0;
    let mut teachers_at_capacity = 0;
    let mut teachers_over_capacity = 0;

    for entry in all_teachers.iter().filter(|t| t.teacher.is_active == 1) {
        let teacher = &entry.teacher;
        let capacity = teacher_capacity(&entry.load, cap);
        total_capacity_hours += capacity;

        let awarded = awarded_by_teacher
            .iter()
            .find(|(id, _)| *id == teacher.id)
            .map(|(_, hours)| *hours)
            .unwrap_or(0);

        if awarded > capacity {
            teachers_over_capacity += 1;
        } else if awarded == capacity && capacity > 0 {
            teachers_at_capacity += 1;
        }
    }

    let assigned_hours = assignments::total_assigned_hours(pool, &term).await?;

    // Havuz TEK hesap noktasından (`teaching_load::total_pool_hours`) okunur;
    // "verilmiş" kullanıcının açıkça seçtiği gibi TAKDİR edilen toplamdır
    // (`company_hours::total_awarded`), atanan DEĞİL.
    let pool_hours = teaching_load::total_pool_hours(pool, &term, as_of).await?;
    let pool_awarded_hours = company_hours::total_awarded(pool, &term).await?;

    Ok(DashboardStats {
        term,
        company_count: all_companies.len() as i64,
        student_count: all_students.len() as i64,
        teacher_count: all_teachers.len() as i64,
        active_teacher_count: all_teachers.iter().filter(|t| t.teacher.is_active == 1).count() as i64,

        total_capacity_hours,
        assigned_hours,
        remaining_hours: total_capacity_hours - assigned_hours,
        teachers_at_capacity,
        teachers_over_capacity,

        pool_hours,
        awarded_hours: pool_awarded_hours,
        remaining_pool_hours: pool_hours - pool_awarded_hours,

        companies_without_location: all_companies
            .iter()
            .filter(|c| c.latitude.is_none() || c.longitude.is_none())
            .count() as i64,
        students_without_company: all_students
            .iter()
            .filter(|s| s.company_id.is_none())
            .count() as i64,
        companies_without_students: all_companies
            .iter()
            .filter(|c| !companies_with_students.contains(&c.id))
            .count() as i64,
        // Öğrencisi olmayan işletme için koordinatör gerekmez; bu yüzden
        // yalnızca öğrencisi olanlar sayılır.
        companies_without_assignment: all_companies
            .iter()
            .filter(|c| companies_with_students.contains(&c.id) && !assigned_companies.contains(&c.id))
            .count() as i64,
    })
}

#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> AppResult<DashboardStats> {
    dashboard_stats(&state.pool).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::db::legacy_seed_test_support::seed_hours;
    use crate::db::teaching_load_test_support::{change_chief_type_in_planning, seed_teacher};
    use crate::domain::models::ChiefType;
    use chrono::NaiveDate;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    /// Kilit test (spec R5c, "üç yer"in biri): Genel Bakış'ın toplam
    /// kapasitesi projeksiyondan okunur. Eski `teachers.chief_type` sütunu
    /// değişmese bile şeflik değişince toplam kapasite buna göre değişmeli.
    #[tokio::test]
    async fn total_capacity_follows_the_projection_not_the_legacy_column() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;

        let before = dashboard_stats(&pool).await.unwrap();
        // Varsayılan ayar (migration 0001): "other" + büyükşehir => tavan 20.
        // Şefsizken bütçe (24) tavanı aştığı için kapasite tavanda KLİPLENİR: 20.
        assert_eq!(before.total_capacity_hours, 20);

        change_chief_type_in_planning(&pool, teacher_id, ChiefType::Department, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).await;

        let after = dashboard_stats(&pool).await.unwrap();
        // Bölüm şefi 10 saat düşürür: bütçe 24-10=14, artık tavanın (20)
        // ALTINDA kaldığı için kapasite tam 14'e düşer (klipleme kalkar).
        assert_eq!(after.total_capacity_hours, 14, "bölüm şefi 10 saat düşürmeli");

        let legacy: String = sqlx::query_scalar("SELECT chief_type FROM teachers WHERE id = ?1")
            .bind(teacher_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(legacy, "none", "eski sütun donuk kalmalı; okuma ona bakmıyor");
    }

    /// Pasif öğretmenin şefliği havuza/kapasiteye girmez (mevcut kural).
    #[tokio::test]
    async fn inactive_teacher_is_excluded_from_active_teacher_count() {
        let (_dir, pool) = test_pool().await;
        seed_teacher(&pool, "Aktif", ChiefType::None).await;
        let passive = seed_teacher(&pool, "Pasif", ChiefType::Department).await;
        sqlx::query("UPDATE teachers SET is_active = 0 WHERE id = ?1")
            .bind(passive)
            .execute(&pool)
            .await
            .unwrap();

        let stats = dashboard_stats(&pool).await.unwrap();
        assert_eq!(stats.teacher_count, 2);
        assert_eq!(stats.active_teacher_count, 1);
    }

    // --- Alan koordinatörlüğü ders yükü havuzu kartı ---

    async fn a_company(pool: &SqlitePool, name: &str) -> i64 {
        companies::create(
            pool,
            &crate::domain::models::NewCompany {
                name: name.into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Test adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(5.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    /// Havuz 160, işletmelere takdir edilen toplam 40 → `poolHours=160`,
    /// `awardedHours=40`, `remainingPoolHours=120`. "Verilmiş", kullanıcının
    /// açıkça seçtiği gibi TAKDİR edilen saattir, atanan DEĞİL.
    #[tokio::test]
    async fn pool_card_reports_pool_awarded_and_remaining() {
        let (_dir, pool) = test_pool().await;
        teaching_load::replace_for_term(
            &pool,
            crate::db::teaching_load_test_support::TERM,
            &[teaching_load::TermBranchHoursInput {
                grade: "12/C".into(),
                branch: "Dal".into(),
                weekly_hours: 160,
                group_count: 1,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();
        let company_id = a_company(&pool, "İşletme A").await;
        seed_hours(&pool, crate::db::teaching_load_test_support::TERM, company_id, 40, false).await;

        let stats = dashboard_stats(&pool).await.unwrap();

        assert_eq!(stats.pool_hours, 160);
        assert_eq!(stats.awarded_hours, 40);
        assert_eq!(stats.remaining_pool_hours, 120);
    }

    /// Takdir havuzu aşarsa (göçten kalma veri) `remainingPoolHours` negatif
    /// döner — kırpma YOKTUR, ekran bunu uyarı olarak kullanır.
    #[tokio::test]
    async fn remaining_pool_hours_goes_negative_when_awarded_exceeds_the_pool() {
        let (_dir, pool) = test_pool().await;
        teaching_load::replace_for_term(
            &pool,
            crate::db::teaching_load_test_support::TERM,
            &[teaching_load::TermBranchHoursInput {
                grade: "12/C".into(),
                branch: "Dal".into(),
                weekly_hours: 40,
                group_count: 1,
                is_group_manual: true,
            }],
        )
        .await
        .unwrap();
        let company_id = a_company(&pool, "İşletme A").await;
        seed_hours(&pool, crate::db::teaching_load_test_support::TERM, company_id, 60, false).await;

        let stats = dashboard_stats(&pool).await.unwrap();

        assert_eq!(stats.remaining_pool_hours, -20);
    }

    /// Fahri satırlar `awardedHours`'a katkı vermez — İş 1'deki mevcut
    /// davranışla (bkz. `company_hours::total_awarded`) AYNI kural.
    #[tokio::test]
    async fn honorary_rows_do_not_count_toward_the_dashboard_awarded_hours() {
        let (_dir, pool) = test_pool().await;
        let paid = a_company(&pool, "Ucretli").await;
        let free = a_company(&pool, "Fahri").await;
        seed_hours(&pool, crate::db::teaching_load_test_support::TERM, paid, 6, false).await;
        seed_hours(&pool, crate::db::teaching_load_test_support::TERM, free, 8, true).await;

        let stats = dashboard_stats(&pool).await.unwrap();

        assert_eq!(stats.awarded_hours, 6, "fahri satırın 8 saati sayılmamalı");
    }
}
