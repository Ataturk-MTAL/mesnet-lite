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
