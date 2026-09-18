mod commands;
mod db;
mod domain;
mod error;
mod services;

use db::{init_pool, AppState};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Veritabanı platform-doğru uygulama veri dizininde tutulur.
            let dir = app.path().app_data_dir()?;
            let db_path = dir.join("mesnet-lite.db");

            let pool = tauri::async_runtime::block_on(init_pool(&db_path))
                .map_err(|e| format!("Veritabanı açılamadı: {e}"))?;

            app.manage(AppState { pool });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Uygulama başlatılamadı");
}
