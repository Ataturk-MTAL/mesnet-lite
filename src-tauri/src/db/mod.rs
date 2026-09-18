use crate::error::{AppError, AppResult};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;

pub mod companies;
pub mod students;
pub mod teachers;

/// Tauri yönetilen durumu. Komutlar veritabanı havuzuna buradan erişir.
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
            "companies",
            "students",
            "teachers",
            "teacher_availability",
            "class_workplace_days",
            "company_hour_rules",
            "assignments",
            "assignment_slots",
            "settings",
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
