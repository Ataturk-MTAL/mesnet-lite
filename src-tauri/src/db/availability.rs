use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Bir öğretmenin BOŞ olduğu tek bir saat dilimi.
/// Satır varsa o saat boştur; satır yoksa dolu kabul edilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilitySlot {
    pub teacher_id: i64,
    /// 1 = Pazartesi … 5 = Cuma
    pub day_of_week: i64,
    pub hour: i64,
}

/// Bir öğretmenin verilen dönemdeki boş saatleri (SON durum).
///
/// Okuma `teacher_schedule_periods`'ın AÇIK aralığından (`valid_to IS NULL`)
/// yapılır: kullanıcı kararı "her halükarda son düzenleme etkin" — gelecek
/// tarihli bir değişiklik de o öğretmenin son durumudur. Tarih itibarıyla
/// okuma (`as_of`) bu işte yoktur. Programı olmayan öğretmen için boş liste
/// döner (hata değil): "boş saat girilmemiş" geçerli bir durumdur.
pub async fn list_for_teacher(
    pool: &SqlitePool,
    teacher_id: i64,
    term: &str,
) -> AppResult<Vec<AvailabilitySlot>> {
    Ok(sqlx::query_as::<_, AvailabilitySlot>(&format!(
        "{OPEN_SLOTS_SQL} AND p.teacher_id = ?2 ORDER BY day_of_week, hour"
    ))
    .bind(term)
    .bind(teacher_id)
    .fetch_all(pool)
    .await?)
}

/// Dönemdeki tüm öğretmenlerin boş saatleri. Dağıtım motoru, atama panosu ve
/// müsaitlik ekranı aynı okuyucudan beslenir; böylece biri eski, biri yeni
/// veriye bakamaz.
pub async fn list_all(pool: &SqlitePool, term: &str) -> AppResult<Vec<AvailabilitySlot>> {
    Ok(sqlx::query_as::<_, AvailabilitySlot>(&format!(
        "{OPEN_SLOTS_SQL} ORDER BY teacher_id, day_of_week, hour"
    ))
    .bind(term)
    .fetch_all(pool)
    .await?)
}

/// `slots_json` (`[[gün, saat], ...]`, bkz. `EventPayload::ScheduleSet`)
/// SQLite'ın `json_each`'iyle satırlara açılır; böylece dönüş biçimi
/// (`teacher_id, day_of_week, hour`) eski tablodakiyle aynı kalır.
///
/// `teachers` ile birleştirme bilinçlidir: projeksiyon tablosunda öğretmene
/// FK yoktur, eski tablodaki `ON DELETE CASCADE`'in karşılığı burada
/// sağlanır — silinen öğretmenin programı görünmez.
/// `?1` her zaman dönemdir.
const OPEN_SLOTS_SQL: &str = "SELECT DISTINCT
        p.teacher_id AS teacher_id,
        CAST(json_extract(slot.value, '$[0]') AS INTEGER) AS day_of_week,
        CAST(json_extract(slot.value, '$[1]') AS INTEGER) AS hour
     FROM teacher_schedule_periods p
     JOIN teachers t ON t.id = p.teacher_id
     JOIN json_each(p.slots_json) slot
     WHERE p.term = ?1 AND p.valid_to IS NULL";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::db::teaching_load_test_support::{seed_teacher, TERM};
    use crate::db::teachers;
    use crate::domain::history::decide::{ChangeCommand, ChangeRequest};
    use crate::domain::models::ChiefType;
    use crate::domain::scheduling::Slot;
    use crate::services::change_service::{execute_change, ChangeMode, ChangeOutcome};
    use chrono::NaiveDate;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Dönem 2026-09-01'de başlar; bu gün planlama evresidir.
    fn planning_today() -> NaiveDate {
        ymd(2026, 8, 15)
    }

    /// Dönem başladı; ay penceresi 1 Kasım'dan açık (spec §5.1).
    fn november_today() -> NaiveDate {
        ymd(2026, 11, 10)
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    async fn a_teacher(pool: &SqlitePool, first_name: &str) -> i64 {
        seed_teacher(pool, first_name, ChiefType::None).await
    }

    fn schedule_request(teacher_id: i64, slots: &[(i64, i64)], effective: Option<NaiveDate>) -> ChangeRequest {
        ChangeRequest {
            term: TERM.to_string(),
            effective_date: effective,
            document_date: None,
            reason: "test".to_string(),
            command: ChangeCommand::SetTeacherSchedule {
                teacher_id,
                slots: slots.iter().map(|&(day, hour)| Slot::new(day, hour)).collect(),
            },
        }
    }

    /// Programı GERÇEK yazma yolundan (`execute_change`) kaydeder ve
    /// değişiklik kümesinin kimliğini döndürür.
    async fn set_schedule(
        pool: &SqlitePool,
        teacher_id: i64,
        slots: &[(i64, i64)],
        effective: Option<NaiveDate>,
        today: NaiveDate,
    ) -> i64 {
        let req = schedule_request(teacher_id, slots, effective);
        let outcome = execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, today)
            .await
            .unwrap();
        match outcome {
            ChangeOutcome::Committed { change_set_id, .. } => change_set_id,
            other => panic!("Committed beklenirdi, gelen: {other:?}"),
        }
    }

    async fn revoke(pool: &SqlitePool, change_set_id: i64, today: NaiveDate) {
        let req = ChangeRequest {
            term: TERM.to_string(),
            effective_date: None,
            document_date: None,
            reason: "geri al".to_string(),
            command: ChangeCommand::Revoke { change_set_id },
        };
        let outcome = execute_change(pool, req, ChangeMode::Commit { expected_high_water: None }, today)
            .await
            .unwrap();
        assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "geri alma reddedildi: {outcome:?}");
    }

    fn pairs(slots: &[AvailabilitySlot]) -> Vec<(i64, i64)> {
        slots.iter().map(|s| (s.day_of_week, s.hour)).collect()
    }

    /// Asıl regresyon: olay yoluyla kaydedilen program okunabilmeli. Okuyucu
    /// eski `teacher_availability` tablosuna baktığı sürece bu test düşer ve
    /// kullanıcı "kaydettiklerim temizleniyor" der.
    #[tokio::test]
    async fn schedule_saved_through_the_change_log_is_read_back() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        set_schedule(&pool, teacher_id, &[(3, 14), (1, 9), (1, 10)], None, planning_today()).await;

        let slots = list_for_teacher(&pool, teacher_id, TERM).await.unwrap();
        assert_eq!(pairs(&slots), vec![(1, 9), (1, 10), (3, 14)], "gün, saat sırasıyla");
        assert!(slots.iter().all(|s| s.teacher_id == teacher_id));
    }

    /// Karar: "her halükarda son düzenleme etkin". Sonuncusu gelecek
    /// tarihli olsa bile o öğretmenin son durumudur.
    #[tokio::test]
    async fn the_latest_change_wins_even_when_future_dated() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        set_schedule(&pool, teacher_id, &[(1, 9)], None, planning_today()).await;
        set_schedule(&pool, teacher_id, &[(2, 10)], Some(ymd(2026, 11, 5)), november_today()).await;
        assert_eq!(pairs(&list_for_teacher(&pool, teacher_id, TERM).await.unwrap()), vec![(2, 10)]);

        set_schedule(&pool, teacher_id, &[(4, 11)], Some(ymd(2026, 12, 1)), november_today()).await;
        assert_eq!(pairs(&list_for_teacher(&pool, teacher_id, TERM).await.unwrap()), vec![(4, 11)]);
    }

    /// Sonuncuyu geri almak bir öncekini yeniden son durum yapar.
    #[tokio::test]
    async fn revoking_the_latest_change_restores_the_previous_schedule() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        set_schedule(&pool, teacher_id, &[(1, 9)], None, planning_today()).await;
        let latest = set_schedule(&pool, teacher_id, &[(2, 10)], Some(ymd(2026, 11, 5)), november_today()).await;
        revoke(&pool, latest, november_today()).await;

        assert_eq!(pairs(&list_for_teacher(&pool, teacher_id, TERM).await.unwrap()), vec![(1, 9)]);
    }

    /// Boş liste "hiç boş saati yok" demektir ve geçerli bir son durumdur.
    #[tokio::test]
    async fn an_explicitly_empty_schedule_reads_as_empty() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        set_schedule(&pool, teacher_id, &[(1, 9)], None, planning_today()).await;
        set_schedule(&pool, teacher_id, &[], None, planning_today()).await;

        assert!(list_for_teacher(&pool, teacher_id, TERM).await.unwrap().is_empty());
    }

    /// Öğretmeni olup projeksiyonda programı olmayan dönem boş program döner
    /// (hata değil).
    #[tokio::test]
    async fn teacher_without_a_projection_row_has_an_empty_schedule() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;

        assert!(list_for_teacher(&pool, teacher_id, TERM).await.unwrap().is_empty());
        assert!(list_all(&pool, TERM).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_all_is_scoped_to_term_and_ordered_by_teacher() {
        let (_dir, pool) = test_pool().await;
        let first = a_teacher(&pool, "Bir").await;
        let second = a_teacher(&pool, "Iki").await;

        set_schedule(&pool, second, &[(2, 11), (2, 10)], None, planning_today()).await;
        set_schedule(&pool, first, &[(1, 9)], None, planning_today()).await;

        let all = list_all(&pool, TERM).await.unwrap();
        assert_eq!(
            all,
            vec![
                AvailabilitySlot { teacher_id: first, day_of_week: 1, hour: 9 },
                AvailabilitySlot { teacher_id: second, day_of_week: 2, hour: 10 },
                AvailabilitySlot { teacher_id: second, day_of_week: 2, hour: 11 },
            ]
        );
        assert!(list_all(&pool, "2027-2028/1").await.unwrap().is_empty(), "başka dönemin verisi karışmamalı");
        assert_eq!(list_for_teacher(&pool, first, TERM).await.unwrap().len(), 1);
    }

    /// Öğretmen silinince programı da görünmez olur (eski tablodaki
    /// ON DELETE CASCADE'in karşılığı; projeksiyonda öğretmene FK yoktur).
    #[tokio::test]
    async fn deleted_teacher_schedule_is_not_returned() {
        let (_dir, pool) = test_pool().await;
        let teacher_id = a_teacher(&pool, "Yilmaz").await;
        set_schedule(&pool, teacher_id, &[(1, 9)], None, planning_today()).await;

        teachers::remove(&pool, teacher_id).await.unwrap();

        assert!(list_all(&pool, TERM).await.unwrap().is_empty());
    }

    /// Gerçek veritabanı senaryosu: kullanıcının 0005 şemasında kayıtlı
    /// programı (`teacher_availability`) var; uygulama güncellenince göç
    /// 0006 çalışır (bkz. migrations/0006_history.sql "Adım 3e" ve "Adım 4":
    /// satırlar `schedule_set` açılış olaylarına, oradan
    /// `teacher_schedule_periods` açık aralığına dönüşür). Yeni okuyucu bu
    /// programı kaybettirmemeli. Test 0001-0005'i uygular, eski tabloya
    /// satır yazar, sonra tüm göçleri çalıştırır.
    #[tokio::test]
    async fn schedule_saved_before_migration_0006_survives_it() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use std::borrow::Cow;

        const LAST_LEGACY_VERSION: i64 = 5;

        let dir = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(dir.path().join("legacy.db"))
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();

        let mut legacy = sqlx::migrate!("./migrations");
        let legacy_steps: Vec<_> = legacy.migrations.iter().filter(|m| m.version <= LAST_LEGACY_VERSION).cloned().collect();
        legacy.migrations = Cow::Owned(legacy_steps);
        legacy.run(&pool).await.unwrap();

        sqlx::query("INSERT INTO teachers (first_name, last_name, field) VALUES ('Eski', 'Öğretmen', 'Elektrik')")
            .execute(&pool)
            .await
            .unwrap();
        let teacher_id: i64 = sqlx::query_scalar("SELECT id FROM teachers").fetch_one(&pool).await.unwrap();
        for (day, hour) in [(1, 9), (1, 10), (5, 15)] {
            sqlx::query("INSERT INTO teacher_availability (teacher_id, day_of_week, hour, term) VALUES (?1, ?2, ?3, ?4)")
                .bind(teacher_id)
                .bind(day)
                .bind(hour)
                .bind(TERM)
                .execute(&pool)
                .await
                .unwrap();
        }

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let slots = list_for_teacher(&pool, teacher_id, TERM).await.unwrap();
        assert_eq!(pairs(&slots), vec![(1, 9), (1, 10), (5, 15)]);
        assert_eq!(list_all(&pool, TERM).await.unwrap().len(), 3);
    }
}
