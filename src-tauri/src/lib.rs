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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Veritabanı platform-doğru uygulama veri dizininde tutulur.
            let dir = app.path().app_data_dir()?;
            let db_path = dir.join(db::DB_FILE_NAME);

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
            commands::company_merge_commands::preview_company_merge,
            commands::company_merge_commands::apply_company_merge,
            commands::student_commands::list_students,
            commands::student_commands::list_terms,
            commands::student_commands::create_student,
            commands::student_commands::update_student,
            commands::student_commands::delete_student,
            commands::teacher_commands::list_teachers,
            commands::teacher_commands::list_teachers_with_capacity,
            commands::teacher_commands::create_teacher,
            commands::teacher_commands::update_teacher,
            commands::teacher_commands::delete_teacher,
            commands::settings_commands::get_settings,
            commands::settings_commands::save_settings,
            commands::settings_commands::set_school_location,
            commands::settings_commands::get_known_terms,
            commands::settings_commands::create_term,
            commands::import_commands::preview_csv_import,
            commands::import_commands::apply_csv_import,
            commands::student_list_commands::preview_student_list_import,
            commands::student_list_commands::apply_student_list_import,
            commands::dashboard_commands::get_dashboard_stats,
            commands::export_commands::export_workbook,
            commands::file_commands::save_to_downloads,
            commands::geocoding_commands::geocode_pending_companies,
            commands::report_commands::export_assignment_sheet,
            commands::report_commands::export_visit_lists,
            commands::commission_minutes_commands::export_commission_minutes_pdf,
            commands::commission_minutes_commands::export_commission_minutes_xlsx,
            commands::hours_commands::get_hours_board,
            commands::hours_commands::save_company_hours,
            commands::hours_commands::auto_distribute_hours,
            commands::hours_commands::get_teaching_load_board,
            commands::hours_commands::save_teaching_load,
            commands::assignment_commands::get_assignment_board,
            commands::assignment_commands::propose_assignments,
            commands::assignment_commands::assign_company,
            commands::assignment_commands::unassign_company,
            commands::assignment_commands::clear_assignments,
            commands::availability_commands::get_availability_board,
            commands::availability_commands::save_teacher_availability,
            commands::availability_commands::save_class_days,
            commands::availability_commands::copy_schedule_from_term,
            commands::history_commands::preview_change,
            commands::history_commands::commit_change,
            commands::history_commands::list_history,
            commands::history_commands::get_subject_history,
            commands::term_commands::list_terms_with_dates,
            commands::term_commands::update_term_dates,
            commands::user_commands::has_any_user,
            commands::user_commands::list_users,
            commands::user_commands::create_user,
            commands::user_commands::rename_user,
            commands::user_commands::set_user_pin,
            commands::user_commands::set_user_active,
            commands::user_commands::login,
            commands::backup_commands::backup_status,
            commands::backup_commands::create_backup,
            commands::backup_commands::restore_backup,
        ])
        .run(tauri::generate_context!())
        .expect("Uygulama başlatılamadı");
}
