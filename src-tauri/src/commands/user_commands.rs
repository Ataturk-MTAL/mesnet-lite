use crate::db::{users, AppState};
use crate::error::AppResult;
use tauri::State;

// Bu katman incedir: sınır doğrulaması `db::users` içinde yapılır (`companies`
// deseninin aksine, burada ayrıca tekrarlanacak bir sınır kuralı yok — isim/PIN
// biçimi hem oluşturmada hem düzenlemede aynı, tek yerde: DRY).

/// En az bir kullanıcı var mı? `false` dönerse arayüz ilk kullanıcıyı
/// oluşturan kurulum ekranını gösterir.
#[tauri::command]
pub async fn has_any_user(state: State<'_, AppState>) -> AppResult<bool> {
    users::has_any_user(&state.pool).await
}

/// Pasifler dahil tüm kullanıcılar. Giriş ekranı yalnız aktifleri süzer,
/// Ayarlar hepsini gösterir.
#[tauri::command]
pub async fn list_users(state: State<'_, AppState>) -> AppResult<Vec<users::UserSummary>> {
    users::list(&state.pool).await
}

#[tauri::command]
pub async fn create_user(
    state: State<'_, AppState>,
    name: String,
    pin: String,
) -> AppResult<users::UserSummary> {
    users::create(&state.pool, &name, &pin).await
}

#[tauri::command]
pub async fn rename_user(state: State<'_, AppState>, id: i64, name: String) -> AppResult<()> {
    users::rename(&state.pool, id, &name).await
}

#[tauri::command]
pub async fn set_user_pin(state: State<'_, AppState>, id: i64, pin: String) -> AppResult<()> {
    users::set_pin(&state.pool, id, &pin).await
}

/// Yumuşak silme: kullanıcı listeden kaldırılmaz, yalnız pasifleşir.
#[tauri::command]
pub async fn set_user_active(
    state: State<'_, AppState>,
    id: i64,
    is_active: bool,
) -> AppResult<()> {
    users::set_active(&state.pool, id, is_active).await
}

/// PIN doğruysa `settings.operator_name`i yazar ve `true` döner; yanlışsa
/// hiçbir şey yazmadan `false` döner. Gerçek doğrulama mantığı `db::users`te —
/// burada tekrarlanmaz.
#[tauri::command]
pub async fn login(state: State<'_, AppState>, user_id: i64, pin: String) -> AppResult<bool> {
    users::login(&state.pool, user_id, &pin).await
}
