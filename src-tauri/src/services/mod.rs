pub mod backup;
pub mod change_service;
pub mod commission_minutes;
pub mod commission_minutes_pdf;
pub mod commission_minutes_xlsx;
pub mod company_merge;
pub mod csv_import;
pub mod excel_export;
pub mod geocoding;
pub mod history_delete;
pub mod history_service;
pub mod import_apply;
pub mod pdf_report;
pub mod student_list_apply;
pub mod student_list_import;

mod change_input;

#[cfg(test)]
mod backup_tests;
#[cfg(test)]
mod change_service_p6_tests;
#[cfg(test)]
mod commission_minutes_test_support;
#[cfg(test)]
mod change_service_test_support;
#[cfg(test)]
mod change_service_tests;
#[cfg(test)]
mod company_merge_tests;
#[cfg(test)]
mod history_delete_tests;
#[cfg(test)]
mod history_service_tests;
#[cfg(test)]
mod real_export_fixture;
