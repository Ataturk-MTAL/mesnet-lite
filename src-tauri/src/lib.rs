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
        .invoke_handler(tauri::generate_handler![
            commands::company_commands::list_companies,
            commands::company_commands::get_company,
            commands::company_commands::create_company,
            commands::company_commands::update_company,
            commands::company_commands::delete_company,
            commands::company_commands::set_company_location,
        ])
        .run(tauri::generate_context!())
        .expect("Uygulama başlatılamadı");
}
