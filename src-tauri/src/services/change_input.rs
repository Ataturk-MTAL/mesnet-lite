//! Değişiklik komutunun sınırdaki hazırlığı: girdi doğrulaması ve
//! `decide`'dan ÖNCE, aynı transaction'da yapılan yerinde oluşturma
//! (spec §5 adım 2).
//!
//! Doğrulama, satır açılmadan önce çalışır: geçersiz bir öğrenci/işletme/
//! öğretmen girdisi hiçbir şey yazmadan `AppError::Validation` olur. Aynı
//! kuralların kopyaları `commands/{student,company,teacher}_commands.rs`
//! içinde ÖZEL (`fn validate`) durumdadır ve bu iş için dokunulmaz olduğundan
//! buradan çağrılamaz; birleştirme raporda istenmiştir.

use std::ops::RangeInclusive;

use sqlx::SqliteConnection;

use crate::db::{companies, students, teachers};
use crate::domain::history::decide::{
    ChangeCommand, Materialized, NewStudentInput, NewTeacherProfile, TransferTarget,
};
use crate::domain::history::events::TeacherLoad;
use crate::domain::models::{ChiefType, EmploymentType, NewCompany, NewStudent, NewTeacher};
use crate::error::{AppError, AppResult};

/// Ziyaret günü ve boş saat günü Pazartesi–Cuma'dır (bkz.
/// `availability_commands::save_teacher_availability`, aynı kural).
const WEEKDAYS: RangeInclusive<i64> = 1..=5;
/// Ders saatleri 1'den başlar; üst sınır ayara (`day_end_hour`) bağlıdır ve
/// `decide` tarafından denetlenir.
const FIRST_LESSON_HOUR: i64 = 1;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Validation(message.into())
}

/// `correct` zincirinin içindeki asıl komutu verir. Bir düzeltmenin
/// `replacement`ı yerinde oluşturma ve kaynak dönem açısından asıl komut
/// gibi davranır.
fn innermost(command: &ChangeCommand) -> &ChangeCommand {
    match command {
        ChangeCommand::Correct { replacement, .. } => innermost(replacement),
        other => other,
    }
}

/// Komutun taşıdığı her dış girdiyi doğrular (spec §8 gövdeleri).
pub(super) fn validate_command(command: &ChangeCommand) -> AppResult<()> {
    match command {
        ChangeCommand::CreateStudent { student, .. } => validate_student(student),
        ChangeCommand::TransferStudent { to: TransferTarget::New { company }, .. } => validate_company(company),
        ChangeCommand::CreateTeacher { teacher, load } => {
            validate_teacher_profile(teacher)?;
            validate_load(load)
        }
        ChangeCommand::SetTeacherLoad { load, .. } => validate_load(load),
        ChangeCommand::SetCompanyHours { rows } => validate_hours(rows.iter().map(|r| r.awarded_hours)),
        ChangeCommand::AssignCoordinators { rows } => rows.iter().try_for_each(|r| validate_slot(r.visit_day, r.visit_hour)),
        ChangeCommand::SetTeacherSchedule { slots, .. } => slots.iter().try_for_each(|s| validate_slot(s.day_of_week, s.hour)),
        ChangeCommand::Correct { replacement, .. } => validate_command(replacement),
        _ => Ok(()),
    }
}

fn validate_student(student: &NewStudentInput) -> AppResult<()> {
    if student.first_name.trim().is_empty() || student.last_name.trim().is_empty() {
        return Err(invalid("Ad ve soyad boş olamaz"));
    }
    if student.grade.trim().is_empty() {
        return Err(invalid("Sınıf boş olamaz"));
    }
    if student.branch.trim().is_empty() {
        return Err(invalid("Dal boş olamaz"));
    }
    Ok(())
}

fn validate_company(company: &NewCompany) -> AppResult<()> {
    if company.name.trim().is_empty() {
        return Err(invalid("İşletme adı boş olamaz"));
    }
    if company.address_text.trim().is_empty() {
        return Err(invalid("Adres boş olamaz"));
    }
    if company.one_way_distance_km.is_some_and(|km| km < 0.0) {
        return Err(invalid("Mesafe negatif olamaz"));
    }
    if company.latitude.is_some_and(|lat| !(-90.0..=90.0).contains(&lat)) {
        return Err(invalid("Enlem -90 ile 90 arasında olmalı"));
    }
    if company.longitude.is_some_and(|lon| !(-180.0..=180.0).contains(&lon)) {
        return Err(invalid("Boylam -180 ile 180 arasında olmalı"));
    }
    Ok(())
}

fn validate_teacher_profile(teacher: &NewTeacherProfile) -> AppResult<()> {
    if teacher.first_name.trim().is_empty() || teacher.last_name.trim().is_empty() {
        return Err(invalid("Ad ve soyad boş olamaz"));
    }
    if teacher.field.trim().is_empty() {
        return Err(invalid("Alan boş olamaz"));
    }
    Ok(())
}

/// MADDE 6/1-c: azamî ek ders tavanı; şeflik saati (MADDE 6/4) bu tavanın
/// İÇİNDE verilir, bu yüzden şeflik saatinden küçük bir tavan tutarsızdır.
fn validate_load(load: &TeacherLoad) -> AppResult<()> {
    if load.base_hours < 0 || load.other_extra_hours < 0 {
        return Err(invalid("Saatler negatif olamaz"));
    }
    let chief_hours = load.chief_type.weekly_hours();
    if load.max_extra_hours < chief_hours {
        return Err(invalid(format!(
            "Azami ek ders ({}) şeflik saatinden ({chief_hours}) küçük olamaz",
            load.max_extra_hours
        )));
    }
    Ok(())
}

fn validate_hours(awarded: impl Iterator<Item = i64>) -> AppResult<()> {
    for hours in awarded {
        if hours < 0 {
            return Err(invalid("Saatler negatif olamaz"));
        }
    }
    Ok(())
}

fn validate_slot(day: i64, hour: i64) -> AppResult<()> {
    if !WEEKDAYS.contains(&day) {
        return Err(invalid("Gün Pazartesi ile Cuma arasında olmalı"));
    }
    if hour < FIRST_LESSON_HOUR {
        return Err(invalid("Ders saati 1 veya daha büyük olmalı"));
    }
    Ok(())
}

/// Yerinde oluşturulan satırın kimliklerini toplar. `students.company_id`
/// bilinçli olarak `None` yazılır: yerleşimin tek doğruluk kaynağı
/// `student_placements` projeksiyonudur (spec §4.1); eski sütun `0007`'de
/// düşer ve komutun `companyId`'si günlüğe yerleştirme olayı olarak girer.
pub(super) async fn materialize(conn: &mut SqliteConnection, term: &str, command: &ChangeCommand) -> AppResult<Materialized> {
    match innermost(command) {
        ChangeCommand::CreateStudent { student, .. } => {
            let created = students::create_in(conn, &student_row(term, student)).await?;
            Ok(Materialized { student_id: Some(created.id), ..Materialized::default() })
        }
        ChangeCommand::TransferStudent { to: TransferTarget::New { company }, .. } => {
            let created = companies::create_in(conn, company).await?;
            Ok(Materialized { company_id: Some(created.id), ..Materialized::default() })
        }
        ChangeCommand::CreateTeacher { teacher, load } => {
            let created = teachers::create_in(conn, &teacher_row(teacher, load)).await?;
            Ok(Materialized { teacher_id: Some(created.id), ..Materialized::default() })
        }
        _ => Ok(Materialized::default()),
    }
}

/// `copySchedulesFromTerm`'un kaynak dönemi; yalnız bu komut için doludur.
pub(super) fn source_term(command: &ChangeCommand) -> Option<&str> {
    match innermost(command) {
        ChangeCommand::CopySchedulesFromTerm { from_term } => Some(from_term.as_str()),
        _ => None,
    }
}

fn student_row(term: &str, student: &NewStudentInput) -> NewStudent {
    NewStudent {
        first_name: student.first_name.clone(),
        last_name: student.last_name.clone(),
        student_no: student.student_no.clone(),
        grade: student.grade.clone(),
        branch: student.branch.clone(),
        company_id: None,
        submitted_at: student.submitted_at.clone(),
        term: term.to_string(),
    }
}

fn teacher_row(teacher: &NewTeacherProfile, load: &TeacherLoad) -> NewTeacher {
    NewTeacher {
        first_name: teacher.first_name.clone(),
        last_name: teacher.last_name.clone(),
        registry_no: teacher.registry_no.clone(),
        field: teacher.field.clone(),
        branches: teacher.branches.clone(),
        employment_type: employment_column(load.employment_type).to_string(),
        base_hours: load.base_hours,
        max_extra_hours: load.max_extra_hours,
        other_extra_hours: load.other_extra_hours,
        chief_type: chief_column(load.chief_type).to_string(),
        is_active: teacher.is_active,
    }
}

/// `teachers.chief_type` sütununun CHECK kısıtındaki değerler.
fn chief_column(chief: ChiefType) -> &'static str {
    match chief {
        ChiefType::None => "none",
        ChiefType::WorkshopLab => "workshop_lab",
        ChiefType::Department => "department",
    }
}

/// `teachers.employment_type` sütununun değerleri.
fn employment_column(employment: EmploymentType) -> &'static str {
    match employment {
        EmploymentType::Tenured => "tenured",
        EmploymentType::Contracted => "contracted",
    }
}
