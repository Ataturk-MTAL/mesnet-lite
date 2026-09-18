use crate::error::{AppError, AppResult};
use tauri::Manager;

/// Üretilen bir dosyayı kullanıcının indirilenler klasörüne yazar ve tam yolu döner.
///
/// Neden burada: webview içinden `<a download>` ile dosya indirmek Tauri'de
/// güvenilir değildir; sandbox indirmeyi sessizce engelleyebilir. Baytları
/// üreten servisler (Excel, PDF) diske hiç dokunmaz — yazma tek bir yerde,
/// burada yapılır.
///
/// Aynı adlı dosya varsa üzerine YAZILMAZ; ada sayı eklenir.
#[tauri::command]
pub async fn save_to_downloads(
    app: tauri::AppHandle,
    file_name: String,
    bytes: Vec<u8>,
) -> AppResult<String> {
    let safe_name = sanitize_file_name(&file_name)?;

    let dir = app
        .path()
        .download_dir()
        .map_err(|e| AppError::Io(format!("İndirilenler klasörü bulunamadı: {e}")))?;

    std::fs::create_dir_all(&dir)?;

    let target = unique_path(&dir, &safe_name);
    std::fs::write(&target, bytes)?;

    Ok(target.to_string_lossy().to_string())
}

/// Dosya adından dizin ayıracı ve üst dizin kaçışlarını temizler.
/// Ad backend'den geliyor olsa da yol enjeksiyonuna kapalı olmak
/// sınır doğrulamasının parçasıdır.
fn sanitize_file_name(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Dosya adı boş olamaz".into()));
    }

    let cleaned: String = trimmed
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | '\0'))
        .collect();

    // Yalnızca noktadan ibaret kalan ad reddedilir.
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        return Err(AppError::Validation("Geçersiz dosya adı".into()));
    }

    Ok(cleaned)
}

/// Var olan dosyanın üzerine yazmamak için ada sayı ekler:
/// `rapor.pdf` → `rapor (1).pdf` → `rapor (2).pdf`
fn unique_path(dir: &std::path::Path, file_name: &str) -> std::path::PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = std::path::Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for index in 1..1000 {
        let next = dir.join(format!("{stem} ({index}){extension}"));
        if !next.exists() {
            return next;
        }
    }

    // Bin denemeden sonra zaman damgasıyla ayrıştır.
    dir.join(format!(
        "{stem} ({}){extension}",
        chrono::Utc::now().timestamp()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_path_separators() {
        assert_eq!(sanitize_file_name("rapor.pdf").unwrap(), "rapor.pdf");
        assert_eq!(
            sanitize_file_name("../../etc/passwd").unwrap(),
            "....etcpasswd"
        );
        assert_eq!(sanitize_file_name("a/b\\c.xlsx").unwrap(), "abc.xlsx");
    }

    #[test]
    fn sanitize_rejects_empty_and_dot_only_names() {
        assert!(sanitize_file_name("").is_err());
        assert!(sanitize_file_name("   ").is_err());
        assert!(sanitize_file_name("..").is_err());
        assert!(sanitize_file_name("...").is_err());
    }

    #[test]
    fn sanitize_keeps_turkish_characters() {
        assert_eq!(
            sanitize_file_name("Görevlendirme Çizelgesi.pdf").unwrap(),
            "Görevlendirme Çizelgesi.pdf"
        );
    }

    /// Mevcut dosyanın üzerine yazılmamalı.
    #[test]
    fn unique_path_avoids_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "rapor.pdf");
        assert_eq!(first.file_name().unwrap(), "rapor.pdf");

        std::fs::write(&first, b"x").unwrap();
        let second = unique_path(dir.path(), "rapor.pdf");
        assert_eq!(second.file_name().unwrap(), "rapor (1).pdf");

        std::fs::write(&second, b"x").unwrap();
        let third = unique_path(dir.path(), "rapor.pdf");
        assert_eq!(third.file_name().unwrap(), "rapor (2).pdf");
    }

    /// Uzantısız ad da çalışmalı.
    #[test]
    fn unique_path_handles_names_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "rapor");
        std::fs::write(&first, b"x").unwrap();

        assert_eq!(
            unique_path(dir.path(), "rapor").file_name().unwrap(),
            "rapor (1)"
        );
    }
}
