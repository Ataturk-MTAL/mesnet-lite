//! e-Okul sınıf listesi içe aktarımının önizleme ve uygulama akışı.
//!
//! Ayrıştırma (`student_list_import.rs`) saftır, veritabanına dokunmaz; bu
//! modül önizleme için okur, uygulama için TEK bir `BEGIN IMMEDIATE`
//! transaction'ında okur+yazar (`change_service.rs`'teki "transaction
//! açıkken havuz kullanılmaz" kuralıyla aynı gerekçe).
//!
//! Eşleştirme kuralı JotForm CSV içe aktarımından (`import_apply.rs`)
//! BİLEREK farklıdır: orada dörtlü anahtar (ad+soyad+sınıf+dal) kullanılır,
//! çünkü dal orada güvenilir bir alandır. e-Okul raporlarında dal sütunu
//! bazen hiç yoktur (R020 örnekleri), bu yüzden yedek anahtar üçlüdür
//! (ad+soyad+sınıf) — brief'in açık talimatı.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::students::{self, normalize};
use crate::db::settings;
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, NewStudentInput};
use crate::domain::models::Student;
use crate::error::{AppError, AppResult};
use crate::services::change_service::{self, ChangeMode, ChangeOutcome};
use crate::services::student_list_import::{parse_student_list_xls, ParsedClassList, ParsedStudentRow};

/// Frontend'den gelen tek dosya: ad + ham baytlar. Dosya frontend'de okunup
/// baytlar olarak gönderilir (`csv_import`'un metin sürümünün XLS karşılığı).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentListFile {
    pub name: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    New,
    Unchanged,
    Changed,
    /// Aktif dönemde bu satırın sınıfına kayıtlıydı ama içe aktarılan
    /// dosyada karşılığı bulunamadı; `apply` bu öğrenciyi tarihçe kapısından
    /// (`ChangeCommand::DeleteStudent`) geçirip siler (kullanıcının isteği:
    /// "e-Okul listesinde olmayan öğrenciyi silelim").
    Removed,
}

/// `changed` satırlarda değişiklikten ÖNCEKİ değerler; kullanıcı neyin
/// değiştiğini görebilsin diye. `removed` satırlarda TEK bilgi kaynağı
/// budur (veritabanındaki hâli); dosyada karşılığı olmadığı için üst
/// seviye alanlar (`firstName`/`lastName`/`branch`/`studentNo`) boş/None
/// bırakılır — bkz. `RowPreview` yorumu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousStudentFields {
    pub first_name: String,
    pub last_name: String,
    pub grade: String,
    pub branch: String,
}

/// Üst seviye alanlar (`studentNo`, `firstName`, `lastName`, `branch`)
/// DOSYADAN gelen değerlerdir. `status == Removed` satırlarda dosyada
/// karşılık YOKTUR; bu yüzden bu alanlar sırasıyla `None`/boş metin kalır
/// ve öğrencinin kimliği yalnızca `previous` içinde taşınır. Frontend
/// `removed` satırları `previous` üzerinden okumalı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowPreview {
    pub student_no: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub branch: String,
    pub status: RowStatus,
    pub previous: Option<PreviousStudentFields>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassPreview {
    pub file_name: String,
    pub grade: String,
    pub field_name: String,
    pub rows: Vec<RowPreview>,
    pub new_count: usize,
    pub unchanged_count: usize,
    pub changed_count: usize,
    pub removed_count: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentListPreview {
    pub classes: Vec<ClassPreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentListSummary {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub removed: usize,
    pub warnings: Vec<String>,
}

/// Ayrıştırılmış bir dosyanın adı + içeriği; dosya sırası uyarı ve önizleme
/// gruplaması için korunur.
struct NamedClass {
    file_name: String,
    class: ParsedClassList,
}

/// Tüm dosyalardaki tek bir satır, ait olduğu sınıfın bilgisiyle birlikte.
/// `is_first_occurrence` false ise bu numara DAHA ÖNCE (başka bir dosyada ya
/// da aynı dosyada) görülmüştür — yalnız ilki işlenir (brief: "bir kez işle").
struct FlatRow<'a> {
    grade: &'a str,
    row: &'a ParsedStudentRow,
    is_first_occurrence: bool,
}

fn parse_named_classes(files: &[StudentListFile]) -> AppResult<Vec<NamedClass>> {
    files
        .iter()
        .map(|f| Ok(NamedClass { file_name: f.name.clone(), class: parse_student_list_xls(&f.content)? }))
        .collect()
}

/// Numarası boş satırlar için uyarı üretir (brief: "Numara boşsa ad+soyad+
/// sınıfa düş ve uyarı ekle").
fn missing_number_warnings(classes: &[NamedClass]) -> Vec<String> {
    let mut warnings = Vec::new();
    for class in classes {
        for row in &class.class.rows {
            if row.student_no.is_none() {
                warnings.push(format!(
                    "{}: {} {} için öğrenci numarası boş; ad + soyad + sınıfa göre eşleştirildi.",
                    class.file_name, row.first_name, row.last_name
                ));
            }
        }
    }
    warnings
}

/// Tüm dosyalardaki satırları dosya sırasıyla TEK bir akışa açar ve aynı
/// öğrenci numarasının ikinci/sonraki görülüşlerini işaretler.
fn flatten_rows<'a>(classes: &'a [NamedClass], warnings: &mut Vec<String>) -> Vec<FlatRow<'a>> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut flat = Vec::new();

    for class in classes {
        for row in &class.class.rows {
            let is_first_occurrence = match row.student_no.as_deref().map(normalize).filter(|n| !n.is_empty()) {
                Some(no) => {
                    let first = seen.insert(no);
                    if !first {
                        warnings.push(format!(
                            "Öğrenci numarası {}: birden çok dosyada bulundu; yalnız ilk görülen satır işlendi.",
                            row.student_no.as_deref().unwrap_or_default()
                        ));
                    }
                    first
                }
                None => true,
            };
            flat.push(FlatRow { grade: &class.class.grade, row, is_first_occurrence });
        }
    }
    flat
}

/// Aktif dönem + öğrenci numarasıyla eşleştirir; numara yoksa ad + soyad +
/// sınıf üçlüsüne düşülür (bkz. modül başı yorumu).
fn find_match<'a>(existing: &'a [Student], student_no: Option<&str>, first_name: &str, last_name: &str, grade: &str) -> Option<&'a Student> {
    let has_no = |s: &&Student| s.student_no.as_deref().map(normalize).filter(|n| !n.is_empty()).is_some();

    if let Some(candidate_no) = student_no.map(normalize).filter(|n| !n.is_empty()) {
        return existing
            .iter()
            .find(|s| s.student_no.as_deref().map(normalize).filter(|n| !n.is_empty()).as_deref() == Some(candidate_no.as_str()));
    }

    let key = (normalize(first_name), normalize(last_name), normalize(grade));
    existing
        .iter()
        .find(|s| !has_no(s) && (normalize(&s.first_name), normalize(&s.last_name), normalize(&s.grade)) == key)
}

/// Eşleşen kayıtla dosyadaki değerler arasında ad/soyad/sınıf/dal farkı olup
/// olmadığına bakar.
fn classify_row(matched: Option<&Student>, grade: &str, row: &ParsedStudentRow) -> (RowStatus, Option<PreviousStudentFields>) {
    let Some(student) = matched else {
        return (RowStatus::New, None);
    };

    let changed = student.first_name != row.first_name
        || student.last_name != row.last_name
        || student.grade != grade
        || student.branch != row.branch;

    if !changed {
        return (RowStatus::Unchanged, None);
    }

    (
        RowStatus::Changed,
        Some(PreviousStudentFields {
            first_name: student.first_name.clone(),
            last_name: student.last_name.clone(),
            grade: student.grade.clone(),
            branch: student.branch.clone(),
        }),
    )
}

/// Dal adı değişikliği yüzünden bir (sınıf, dal) çiftinin öğrencisiz
/// kalıp kalmadığını izler (brief "bilinen tuzak": dosyadaki dal adları
/// veritabanındakiyle birebir aynı değil; ders yükü satırları bu metne
/// birebir bağlı olduğu için eski satır sessizce öğrencisiz kalabilir).
struct VacancyTracker {
    /// İçe aktarma SONRASI her (sınıf, dal) çiftinde kaç öğrenci kalacağı.
    occupancy: HashMap<(String, String), i64>,
    /// En az bir öğrencinin TERK ETTİĞİ çiftler; bitişte hâlâ boşsa uyarılır.
    left: HashSet<(String, String)>,
}

impl VacancyTracker {
    fn new(existing: &[Student]) -> Self {
        let mut occupancy = HashMap::new();
        for s in existing {
            if !s.grade.trim().is_empty() && !s.branch.trim().is_empty() {
                *occupancy.entry((s.grade.clone(), s.branch.clone())).or_insert(0) += 1;
            }
        }
        Self { occupancy, left: HashSet::new() }
    }

    fn record(&mut self, matched: Option<&Student>, new_grade: &str, new_branch: &str) {
        let new_pair = (new_grade.to_string(), new_branch.to_string());

        if let Some(student) = matched {
            let old_pair = (student.grade.clone(), student.branch.clone());
            if old_pair == new_pair {
                return;
            }
            if !student.branch.trim().is_empty() {
                if let Some(count) = self.occupancy.get_mut(&old_pair) {
                    *count -= 1;
                }
                self.left.insert(old_pair);
            }
        }

        if !new_branch.trim().is_empty() {
            *self.occupancy.entry(new_pair).or_insert(0) += 1;
        }
    }

    fn finish(self) -> Vec<String> {
        let mut messages: Vec<String> = self
            .left
            .into_iter()
            .filter(|pair| self.occupancy.get(pair).copied().unwrap_or(0) <= 0)
            .map(|(grade, branch)| {
                format!("{grade} · {branch} satırında artık öğrenci yok; ders yükü satırını yeni dal adına göre düzenleyin.")
            })
            .collect();
        messages.sort();
        messages
    }
}

fn empty_class_preview(named: &NamedClass) -> ClassPreview {
    ClassPreview {
        file_name: named.file_name.clone(),
        grade: named.class.grade.clone(),
        field_name: named.class.field_name.clone(),
        rows: Vec::new(),
        new_count: 0,
        unchanged_count: 0,
        changed_count: 0,
        removed_count: 0,
    }
}

/// Bir satırı önizleme kaydına çevirir ve ait olduğu sınıfın sayaçlarını
/// günceller; yinelenen bir numaraysa (`is_first_occurrence == false`) hiçbir
/// şey yapmaz. Eşleşen bir kayıt bulunduysa kimliğini döner — çağıran bunu
/// "dosyada karşılığı olan öğrenciler" kümesine ekler (silme adaylarını
/// bulmak için: bkz. `removal_candidates`).
fn push_row_preview(class: &mut ClassPreview, existing: &[Student], flat_row: &FlatRow, vacancy: &mut VacancyTracker) -> Option<i64> {
    if !flat_row.is_first_occurrence {
        return None;
    }

    let matched = find_match(existing, flat_row.row.student_no.as_deref(), &flat_row.row.first_name, &flat_row.row.last_name, flat_row.grade);
    let matched_id = matched.map(|s| s.id);
    vacancy.record(matched, flat_row.grade, &flat_row.row.branch);
    let (status, previous) = classify_row(matched, flat_row.grade, flat_row.row);

    match status {
        RowStatus::New => class.new_count += 1,
        RowStatus::Unchanged => class.unchanged_count += 1,
        RowStatus::Changed => class.changed_count += 1,
        RowStatus::Removed => unreachable!("classify_row dosyadaki bir satır için hiçbir zaman Removed döndürmez"),
    }
    class.rows.push(RowPreview {
        student_no: flat_row.row.student_no.clone(),
        first_name: flat_row.row.first_name.clone(),
        last_name: flat_row.row.last_name.clone(),
        branch: flat_row.row.branch.clone(),
        status,
        previous,
    });
    matched_id
}

/// Aktif dönemde kayıtlı ama BU İÇE AKTARMANIN kapsadığı sınıflardan birine
/// ait olup (`imported_grades`) dosyalarda karşılığı bulunamayan (`matched_ids`
/// dışında kalan) öğrencileri verir. Kapsam kasıtlı olarak dar tutulur: bir
/// öğrencinin sınıfı içe aktarılan dosyalardan HİÇBİRİNE ait değilse asla aday
/// olmaz — tek bir sınıfın listesini içe aktarmak diğer sınıfların
/// öğrencilerini silmemeli (brief'in en kritik kuralı).
fn removal_candidates<'a>(existing: &'a [Student], imported_grades: &HashSet<&str>, matched_ids: &HashSet<i64>) -> Vec<&'a Student> {
    existing
        .iter()
        .filter(|s| !s.grade.trim().is_empty())
        .filter(|s| imported_grades.contains(s.grade.as_str()))
        .filter(|s| !matched_ids.contains(&s.id))
        .collect()
}

/// Dosyaları ayrıştırıp veritabanıyla karşılaştırır. HİÇBİR ŞEY YAZMAZ.
pub async fn preview(pool: &SqlitePool, files: &[StudentListFile]) -> AppResult<StudentListPreview> {
    let term = settings::get_active_term(pool).await?;
    let named = parse_named_classes(files)?;
    let mut warnings = missing_number_warnings(&named);
    let flat = flatten_rows(&named, &mut warnings);
    let existing = students::list_by_term(pool, &term).await?;

    let mut classes: Vec<ClassPreview> = named.iter().map(empty_class_preview).collect();
    let mut vacancy = VacancyTracker::new(&existing);
    let mut matched_ids: HashSet<i64> = HashSet::new();

    let mut flat_iter = flat.iter();
    for (class_index, named_class) in named.iter().enumerate() {
        for _ in 0..named_class.class.rows.len() {
            let flat_row = flat_iter.next().expect("akış sınıf satır sayısıyla birebir eşleşir");
            if let Some(id) = push_row_preview(&mut classes[class_index], &existing, flat_row, &mut vacancy) {
                matched_ids.insert(id);
            }
        }
    }

    // Silme adayları: her sınıfın önizlemesine, o sınıfı KAPSAYAN İLK dosyanın
    // altında eklenir (aynı sınıfın iki dosyada geçmesi beklenmez ama olursa
    // yinelenmeyi önler).
    let imported_grades: HashSet<&str> = named.iter().map(|n| n.class.grade.as_str()).collect();
    let mut grade_to_class_index: HashMap<&str, usize> = HashMap::new();
    for (index, named_class) in named.iter().enumerate() {
        grade_to_class_index.entry(named_class.class.grade.as_str()).or_insert(index);
    }
    for candidate in removal_candidates(&existing, &imported_grades, &matched_ids) {
        let class_index = grade_to_class_index[candidate.grade.as_str()];
        let class = &mut classes[class_index];
        class.removed_count += 1;
        class.rows.push(RowPreview {
            student_no: None,
            first_name: String::new(),
            last_name: String::new(),
            branch: String::new(),
            status: RowStatus::Removed,
            previous: Some(PreviousStudentFields {
                first_name: candidate.first_name.clone(),
                last_name: candidate.last_name.clone(),
                grade: candidate.grade.clone(),
                branch: candidate.branch.clone(),
            }),
        });
    }

    warnings.extend(vacancy.finish());
    Ok(StudentListPreview { classes, warnings })
}

/// `execute_in`in `Committed` dışındaki sonuçlarını `AppError::Validation`a
/// çevirir. Bu akışta ayrı bir önizleme/bayat kontrolü YOK (`expected_high_water: None`);
/// e-Okul dosyası zaten önizlemede kullanıcıya gösterilmiş kabul edilir.
fn commit_outcome_to_student(outcome: ChangeOutcome) -> AppResult<()> {
    match outcome {
        ChangeOutcome::Committed { .. } => Ok(()),
        ChangeOutcome::Rejected { reason, .. } => Err(AppError::Validation(reason)),
        ChangeOutcome::Stale { message } => Err(AppError::Validation(message)),
        ChangeOutcome::Preview { .. } => {
            unreachable!("ChangeMode::Commit ile çağrıldığında Preview dönmez")
        }
    }
}

/// Yeni bir öğrenciyi `change_service::execute_in` ÜZERİNDEN (tarihçe kapısı,
/// R4) aynı transaction'da oluşturur ve yeni satırı geri okur. İşletme
/// ataması YOK: bu dosyada işletme bilgisi bulunmaz (`company_id: None`).
///
/// `today`, komut sınırında (`commands/student_list_commands.rs`) BİR kez
/// hesaplanıp buraya kadar taşınır (`history_commands.rs`'teki desenin aynısı);
/// böylece testler saat dilimine/gerçek takvime bağlı kalmadan sabit bir
/// "bugün" ile çalışabilir.
async fn create_student_in(
    conn: &mut sqlx::SqliteConnection,
    term: &str,
    grade: &str,
    row: &ParsedStudentRow,
    effective_date: Option<NaiveDate>,
    reason: &str,
    today: NaiveDate,
) -> AppResult<Student> {
    let req = ChangeRequest {
        term: term.to_string(),
        effective_date,
        document_date: None,
        reason: reason.to_string(),
        command: ChangeCommand::CreateStudent {
            student: NewStudentInput {
                first_name: row.first_name.clone(),
                last_name: row.last_name.clone(),
                student_no: row.student_no.clone(),
                grade: grade.to_string(),
                branch: row.branch.clone(),
                submitted_at: None,
            },
            company_id: None,
        },
    };

    let outcome = change_service::execute_in(conn, req, ChangeMode::Commit { expected_high_water: None }, today).await?;
    commit_outcome_to_student(outcome)?;

    let created = students::list_by_term_in(conn, term).await?;
    find_match(&created, row.student_no.as_deref(), &row.first_name, &row.last_name, grade)
        .cloned()
        .ok_or_else(|| AppError::Database("Yeni oluşturulan öğrenci geri okunamadı".into()))
}

/// Dosyalarda karşılığı bulunmayan bir öğrenciyi `change_service::execute_in`
/// ÜZERİNDEN (tarihçe kapısı) aynı transaction'da siler. Kapı reddederse
/// (ör. `HasHistory`: öğrencinin açılış dışında geçmişi var) `Ok(Err(mesaj))`
/// döner; bu durumda çağıran TÜM içe aktarmayı ÇÖKERTMEZ, yalnız o öğrenciyi
/// atlayıp uyarı ekler (kullanıcının isteği: silme geçmişi ezmesin). Yalnız
/// gerçek bir hata (`Err`) — ör. bağlantı sorunu — işlemi durdurur.
async fn delete_student_in(
    conn: &mut sqlx::SqliteConnection,
    term: &str,
    student_id: i64,
    effective_date: Option<NaiveDate>,
    reason: &str,
    today: NaiveDate,
) -> AppResult<Result<(), String>> {
    let req = ChangeRequest {
        term: term.to_string(),
        effective_date,
        document_date: None,
        reason: reason.to_string(),
        command: ChangeCommand::DeleteStudent { student_id },
    };

    match change_service::execute_in(conn, req, ChangeMode::Commit { expected_high_water: None }, today).await? {
        ChangeOutcome::Committed { .. } => Ok(Ok(())),
        ChangeOutcome::Rejected { reason, .. } => Ok(Err(reason)),
        ChangeOutcome::Stale { message } => Ok(Err(message)),
        ChangeOutcome::Preview { .. } => unreachable!("ChangeMode::Commit ile çağrıldığında Preview dönmez"),
    }
}

/// Önizlemede onaylanan içe aktarmayı TEK bir transaction'da uygular.
/// Hata olursa (ör. dönem başladıktan sonra tarih/eşik reddi) hiçbir satır
/// yazılmaz — transaction commit edilmeden düşer, sqlx otomatik geri alır.
pub async fn apply(
    pool: &SqlitePool,
    files: &[StudentListFile],
    effective_date: Option<NaiveDate>,
    reason: &str,
    today: NaiveDate,
) -> AppResult<StudentListSummary> {
    let term = settings::get_active_term(pool).await?;
    let named = parse_named_classes(files)?;
    let mut warnings = missing_number_warnings(&named);
    let flat = flatten_rows(&named, &mut warnings);

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut known = students::list_by_term_in(&mut tx, &term).await?;
    // Silme adaylarını, bu transaction'da OLUŞTURULAN öğrencilerden ayırmak
    // için işlem başlamadan ÖNCEki veritabanı görüntüsü ayrıca saklanır.
    let existing_at_start = known.clone();
    let mut vacancy = VacancyTracker::new(&known);
    let mut summary = StudentListSummary::default();
    let mut matched_ids: HashSet<i64> = HashSet::new();

    for flat_row in &flat {
        if !flat_row.is_first_occurrence {
            summary.skipped += 1;
            continue;
        }

        let matched = find_match(&known, flat_row.row.student_no.as_deref(), &flat_row.row.first_name, &flat_row.row.last_name, flat_row.grade).cloned();
        vacancy.record(matched.as_ref(), flat_row.grade, &flat_row.row.branch);

        match matched {
            None => {
                let created = create_student_in(&mut tx, &term, flat_row.grade, flat_row.row, effective_date, reason, today).await?;
                known.push(created);
                summary.created += 1;
            }
            Some(existing) => {
                matched_ids.insert(existing.id);
                let (status, _) = classify_row(Some(&existing), flat_row.grade, flat_row.row);
                if status == RowStatus::Changed {
                    students::update_identity_fields_in(
                        &mut tx,
                        existing.id,
                        &flat_row.row.first_name,
                        &flat_row.row.last_name,
                        flat_row.grade,
                        &flat_row.row.branch,
                    )
                    .await?;
                    summary.updated += 1;
                } else {
                    summary.skipped += 1;
                }
            }
        }
    }

    let imported_grades: HashSet<&str> = named.iter().map(|n| n.class.grade.as_str()).collect();
    for candidate in removal_candidates(&existing_at_start, &imported_grades, &matched_ids) {
        match delete_student_in(&mut tx, &term, candidate.id, effective_date, reason, today).await? {
            Ok(()) => summary.removed += 1,
            Err(rejection_reason) => warnings.push(format!(
                "{} {} ({}) e-Okul listesinde artık yok ama silinemedi: {}",
                candidate.first_name, candidate.last_name, candidate.grade, rejection_reason
            )),
        }
    }

    tx.commit().await?;
    warnings.extend(vacancy.finish());
    summary.warnings = warnings;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{companies, init_pool};
    use crate::domain::models::NewCompany;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    fn read_fixture(name_fragment: &str) -> StudentListFile {
        let data_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("data");
        let path = std::fs::read_dir(&data_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xls"))
                    && p.file_name().unwrap().to_string_lossy().contains(name_fragment)
            })
            .unwrap_or_else(|| panic!("fixture bulunamadı: {name_fragment}"));
        StudentListFile { name: path.file_name().unwrap().to_string_lossy().to_string(), content: std::fs::read(path).unwrap() }
    }

    fn r076_files() -> Vec<StudentListFile> {
        vec![read_fixture("R076_920 (1).XLS"), read_fixture("R076_920.XLS")]
    }

    /// Varsayılan tohum dönemi ("2026-2027/1") 1 Eylül 2026'da başlar
    /// (bkz. migration 0006 göç tohumu). Bu tarih, dönem HENÜZ BAŞLAMADAN
    /// ÖNCEki (planlama evresi) "bugün"dür: `effectiveDate` boş bırakılabilir.
    fn before_term_start() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 15).unwrap()
    }

    /// Aynı varsayılan dönemin BAŞLADIKTAN SONRAki bir "bugün"ü: bu noktadan
    /// itibaren `effectiveDate` zorunludur (MADDE 5/1-ç tarihçe kapısı).
    fn after_term_start() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 20).unwrap()
    }

    /// Boş veritabanında önizleme her satırı `new` göstermeli; 34 öğrenci.
    #[tokio::test]
    async fn preview_on_empty_database_marks_every_row_new() {
        let (_dir, pool) = test_pool().await;
        let preview_result = preview(&pool, &r076_files()).await.unwrap();

        let total_rows: usize = preview_result.classes.iter().map(|c| c.rows.len()).sum();
        assert_eq!(total_rows, 34);
        assert!(preview_result.classes.iter().all(|c| c.changed_count == 0 && c.unchanged_count == 0));
        assert!(preview_result.classes.iter().all(|c| c.rows.iter().all(|r| r.status == RowStatus::New)));
    }

    /// Uygulama: 34 öğrenci oluşur, işletme ataması yok, ikinci uygulama
    /// yeni öğrenci yaratmaz (önizleme "unchanged" gösterir).
    #[tokio::test]
    async fn apply_creates_all_students_once_and_is_idempotent() {
        let (_dir, pool) = test_pool().await;
        let files = r076_files();

        let summary = apply(&pool, &files, None, "e-Okul sınıf listesi içe aktarımı", before_term_start()).await.unwrap();
        assert_eq!(summary.created, 34);
        assert_eq!(summary.updated, 0);

        let all = students::list(&pool).await.unwrap();
        assert_eq!(all.len(), 34);
        assert!(all.iter().all(|s| s.company_id.is_none()), "e-Okul listesinde işletme bilgisi yok");

        // change_events'te CreateStudent olayları var mı: change_sets.kind = 'create_student'.
        let created_kind_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_sets WHERE kind = 'create_student'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(created_kind_count, 34);

        let preview_after = preview(&pool, &files).await.unwrap();
        assert!(preview_after.classes.iter().all(|c| c.new_count == 0 && c.changed_count == 0));

        let second = apply(&pool, &files, None, "tekrar", before_term_start()).await.unwrap();
        assert_eq!(second.created, 0);
        assert_eq!(second.updated, 0);
        assert_eq!(second.skipped, 34);
        assert_eq!(students::list(&pool).await.unwrap().len(), 34);
    }

    /// Bir öğrencinin dalı elle değiştirilip tekrar önizlenince `changed` ve
    /// `previous` dolu görünmeli.
    #[tokio::test]
    async fn preview_reports_changed_status_with_previous_values() {
        let (_dir, pool) = test_pool().await;
        let files = r076_files();
        apply(&pool, &files, None, "ilk aktarım", before_term_start()).await.unwrap();

        let all = students::list(&pool).await.unwrap();
        let first_row = all.iter().find(|s| s.student_no.as_deref() == Some("9001")).unwrap();
        let mut updated_input = crate::domain::models::NewStudent {
            first_name: first_row.first_name.clone(),
            last_name: first_row.last_name.clone(),
            student_no: first_row.student_no.clone(),
            grade: first_row.grade.clone(),
            branch: "Elden Değiştirilmiş Dal".to_string(),
            company_id: first_row.company_id,
            submitted_at: first_row.submitted_at.clone(),
            term: first_row.term.clone(),
        };
        updated_input.branch = "Elden Değiştirilmiş Dal".to_string();
        students::update(&pool, first_row.id, &updated_input).await.unwrap();

        let preview_result = preview(&pool, &files).await.unwrap();
        let class_c = preview_result.classes.iter().find(|c| c.grade == "12/C").unwrap();
        let row = class_c.rows.iter().find(|r| r.student_no.as_deref() == Some("9001")).unwrap();

        assert_eq!(row.status, RowStatus::Changed);
        let previous = row.previous.as_ref().unwrap();
        assert_eq!(previous.branch, "Elden Değiştirilmiş Dal");
        assert_eq!(row.branch, "Elektronik ve Haberleşme");
    }

    /// Aynı öğrenci numarası iki dosyada varsa uyarı verilir ve yalnız bir
    /// kez işlenir.
    #[tokio::test]
    async fn duplicate_student_number_across_files_is_processed_once() {
        let (_dir, pool) = test_pool().await;
        let mut files = r076_files();
        let duplicate_of_first = files[0].clone();
        files.push(duplicate_of_first);

        let summary = apply(&pool, &files, None, "test", before_term_start()).await.unwrap();
        assert_eq!(summary.created, 34, "aynı dosya iki kez eklendi ama yalnız 34 tekil öğrenci var");
        assert!(summary.warnings.iter().any(|w| w.contains("birden çok dosyada bulundu")));
        assert_eq!(students::list(&pool).await.unwrap().len(), 34);
    }

    /// e-Okul'daki dal adı veritabanındakiyle birebir aynı değilse (brief
    /// "bilinen tuzak"), o (sınıf, dal) çiftini kullanan tek öğrenci
    /// taşındığında ders yükü satırının öğrencisiz kaldığı uyarılmalı.
    #[tokio::test]
    async fn preview_warns_when_a_branch_rename_empties_the_old_teaching_load_row() {
        let (_dir, pool) = test_pool().await;
        let term = settings::get_active_term(&pool).await.unwrap();

        // Veritabanı kuralı DB'nin kendi adlandırmasıyla (dosyadakinden farklı) önceden var.
        students::create(
            &pool,
            &crate::domain::models::NewStudent {
                first_name: "ALVAR".into(),
                last_name: "QUENNET".into(),
                student_no: Some("9001".into()),
                grade: "12/C".into(),
                branch: "Elektronik Haberleşme".into(),
                company_id: None,
                submitted_at: None,
                term,
            },
        )
        .await
        .unwrap();

        let preview_result = preview(&pool, &[read_fixture("R076_920 (1).XLS")]).await.unwrap();
        assert!(
            preview_result.warnings.iter().any(|w| w.contains("12/C") && w.contains("Elektronik Haberleşme") && w.contains("öğrenci yok")),
            "uyarı bulunamadı: {:?}",
            preview_result.warnings
        );
    }

    /// Dönem başlamışken tarih verilmezse tarihçe kapısı reddeder (MADDE 5/1-ç
    /// yürürlük tarihi zorunluluğu, `domain::terms::resolve_effective_date`);
    /// bu red `AppError::Validation`a çevrilmeli ve HİÇBİR ŞEY yazılmamalı.
    #[tokio::test]
    async fn apply_after_term_started_without_effective_date_is_rejected() {
        let (_dir, pool) = test_pool().await;

        let result = apply(&pool, &r076_files(), None, "gerekçe", after_term_start()).await;
        assert!(matches!(result, Err(AppError::Validation(_))), "beklenmeyen sonuç: {result:?}");
        assert_eq!(students::list(&pool).await.unwrap().len(), 0, "reddedilen işlem hiçbir şey yazmamalı");
    }

    /// BİLİNEN ÇELİŞKİ (rapora yazıldı): R020 tipi raporlarda "Dalı" sütunu
    /// yoktur, ama `change_input::validate_student` (MADDE 5/1-ç'nin
    /// dayandığı girdi doğrulaması) YENİ bir öğrencide boş dalı reddeder.
    /// Bu, dal sütunu olmayan bir e-Okul raporunu YENİ öğrenci olarak içe
    /// aktarmanın bugün mümkün OLMADIĞINI kanıtlar; kural bilerek
    /// gevşetilmedi (brief: "kuralı kendi başına gevşetme").
    #[tokio::test]
    async fn applying_a_branchless_file_as_new_students_is_rejected_by_existing_domain_rule() {
        let (_dir, pool) = test_pool().await;
        let branchless = vec![read_fixture("R020_920 (1).XLS")];

        let result = apply(&pool, &branchless, Some(before_term_start()), "test", before_term_start()).await;

        let Err(AppError::Validation(message)) = result else {
            panic!("beklenmeyen sonuç: {result:?}");
        };
        assert!(message.contains("Dal boş olamaz"), "mesaj: {message}");
        assert_eq!(students::list(&pool).await.unwrap().len(), 0, "reddedilen işlem hiçbir şey yazmamalı");
    }

    /// Bir öğrenci elle eklenip dosyada karşılığı olmayan bir kayıt olarak
    /// bırakılır: önizlemede `removed` görünmeli (yalnız `previous` dolu, üst
    /// alanlar boş/None — `RowPreview` yorumu), uygulanınca silinmeli ve
    /// `change_sets`'te bir `delete_student` kaydı oluşmalı. Kullanıcının
    /// isteği: "e-Okul listesinde olmayan öğrenciyi silelim, geçmişe kaydedelim".
    #[tokio::test]
    async fn preview_and_apply_remove_a_student_missing_from_the_imported_class() {
        let (_dir, pool) = test_pool().await;
        let files = r076_files();
        apply(&pool, &files, None, "ilk aktarım", before_term_start()).await.unwrap();

        let term = settings::get_active_term(&pool).await.unwrap();
        students::create(
            &pool,
            &crate::domain::models::NewStudent {
                first_name: "FAZLA".into(),
                last_name: "ÖĞRENCİ".into(),
                student_no: Some("9999".into()),
                grade: "12/C".into(),
                branch: "Elektronik ve Haberleşme".into(),
                company_id: None,
                submitted_at: None,
                term,
            },
        )
        .await
        .unwrap();

        let preview_result = preview(&pool, &files).await.unwrap();
        let class_c = preview_result.classes.iter().find(|c| c.grade == "12/C").unwrap();
        assert_eq!(class_c.removed_count, 1);
        let removed_row = class_c.rows.iter().find(|r| r.status == RowStatus::Removed).unwrap();
        assert_eq!(removed_row.student_no, None, "removed satırda dosyadan gelen alan yok");
        assert_eq!(removed_row.first_name, "", "removed satırda üst alan boş bırakılır");
        let previous = removed_row.previous.as_ref().expect("removed satırda previous dolu olmalı");
        assert_eq!(previous.first_name, "FAZLA");
        assert_eq!(previous.last_name, "ÖĞRENCİ");
        assert_eq!(previous.grade, "12/C");

        let summary = apply(&pool, &files, None, "temizlik", before_term_start()).await.unwrap();
        assert_eq!(summary.removed, 1);
        assert_eq!(students::list(&pool).await.unwrap().len(), 34, "fazlalık öğrenci silindi, 34 kaldı");

        let deleted_kind_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_sets WHERE kind = 'delete_student'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(deleted_kind_count, 1);

        // İkinci uygulama artık silecek bir şey bulamaz (idempotentlik).
        let second = apply(&pool, &files, None, "tekrar", before_term_start()).await.unwrap();
        assert_eq!(second.removed, 0, "zaten silinmiş öğrenci ikinci kez silinmez");
    }

    /// KAPSAM KORUMASI (brief'in en kritik kuralı): yalnız 12/C dosyası
    /// içe aktarılınca 12/D öğrencileri SİLİNMEZ; silme yalnızca içe
    /// aktarılan dosyaların kapsadığı sınıflarla sınırlıdır.
    #[tokio::test]
    async fn importing_only_one_class_file_does_not_delete_the_other_class() {
        let (_dir, pool) = test_pool().await;
        let files = r076_files(); // 12/C (17) + 12/D (17)
        apply(&pool, &files, None, "ilk aktarım", before_term_start()).await.unwrap();
        assert_eq!(students::list(&pool).await.unwrap().len(), 34);

        // Yalnız 12/C dosyası TEKRAR uygulanır; 12/D bu içe aktarmanın
        // kapsamında değildir ve dokunulmamalıdır.
        let only_c = vec![read_fixture("R076_920 (1).XLS")];
        let summary = apply(&pool, &only_c, None, "yalnız 12/C", before_term_start()).await.unwrap();

        assert_eq!(summary.removed, 0, "12/C dosyasındaki tüm öğrenciler zaten kayıtlı");
        let all = students::list(&pool).await.unwrap();
        assert_eq!(all.len(), 34, "12/D öğrencilerine dokunulmamalı");
        assert_eq!(all.iter().filter(|s| s.grade == "12/D").count(), 17, "12/D sınıfı tam kalmalı");
    }

    /// Sınıfı boş olan ya da dosyaların kapsadığı sınıflardan hiçbirine
    /// eşleşmeyen bir öğrenci, dosyada karşılığı olmasa bile SİLİNMEZ.
    #[tokio::test]
    async fn students_outside_the_imported_grades_are_never_removal_candidates() {
        let (_dir, pool) = test_pool().await;
        let files = r076_files();
        let term = settings::get_active_term(&pool).await.unwrap();

        students::create(
            &pool,
            &crate::domain::models::NewStudent {
                first_name: "BOŞ".into(),
                last_name: "SINIF".into(),
                student_no: Some("1001".into()),
                grade: "".into(),
                branch: "".into(),
                company_id: None,
                submitted_at: None,
                term: term.clone(),
            },
        )
        .await
        .unwrap();
        students::create(
            &pool,
            &crate::domain::models::NewStudent {
                first_name: "BAŞKA".into(),
                last_name: "SINIF".into(),
                student_no: Some("1002".into()),
                grade: "11/A".into(),
                branch: "Elektronik ve Haberleşme".into(),
                company_id: None,
                submitted_at: None,
                term,
            },
        )
        .await
        .unwrap();

        let summary = apply(&pool, &files, None, "test", before_term_start()).await.unwrap();
        assert_eq!(summary.removed, 0, "sınıfı kapsam dışı olan öğrenciler silinmez");
        assert_eq!(students::list(&pool).await.unwrap().len(), 34 + 2);
    }

    /// Kapı (`HasHistory`) silmeyi reddederse TÜM içe aktarma çökmez: o
    /// öğrenci atlanır, `warnings`'e ad + red gerekçesiyle açık bir satır
    /// eklenir, aynı taramadaki DİĞER silinebilir öğrenci yine de silinir.
    #[tokio::test]
    async fn a_student_the_gate_refuses_to_delete_does_not_abort_the_import() {
        let (_dir, pool) = test_pool().await;
        let only_c = vec![read_fixture("R076_920 (1).XLS")];
        apply(&pool, &only_c, None, "ilk aktarım", before_term_start()).await.unwrap();

        let term = settings::get_active_term(&pool).await.unwrap();
        let company = companies::create(
            &pool,
            &NewCompany {
                name: "Tarihi İşletme".into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Örnek Mah.".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(3.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        // Geçmişi olan öğrenci: oluşturulurken bir işletmeye yerleştirilir —
        // bu, `create_student` kümesiyle canlı (açılış olmayan) bir yerleşim
        // olayı yaratır ve `delete_student`'ın `HasHistory` reddine yol açar
        // (bkz. `domain::history::decide::student::delete_student`).
        let outcome = change_service::execute_change(
            &pool,
            ChangeRequest {
                term: term.clone(),
                effective_date: None,
                document_date: None,
                reason: "test".into(),
                command: ChangeCommand::CreateStudent {
                    student: NewStudentInput {
                        first_name: "GEÇMİŞLİ".into(),
                        last_name: "ÖĞRENCİ".into(),
                        student_no: Some("9997".into()),
                        grade: "12/C".into(),
                        branch: "Elektronik ve Haberleşme".into(),
                        submitted_at: None,
                    },
                    company_id: Some(company.id),
                },
            },
            ChangeMode::Commit { expected_high_water: None },
            before_term_start(),
        )
        .await
        .unwrap();
        assert!(matches!(outcome, ChangeOutcome::Committed { .. }), "sahne kurulamadı: {outcome:?}");

        // Aynı sınıfta, geçmişi olmayan, silinmesi gereken bir öğrenci daha.
        students::create(
            &pool,
            &crate::domain::models::NewStudent {
                first_name: "SİLİNECEK".into(),
                last_name: "ÖĞRENCİ".into(),
                student_no: Some("9996".into()),
                grade: "12/C".into(),
                branch: "Elektronik ve Haberleşme".into(),
                company_id: None,
                submitted_at: None,
                term,
            },
        )
        .await
        .unwrap();

        let summary = apply(&pool, &only_c, None, "temizlik", before_term_start()).await.unwrap();

        assert_eq!(summary.removed, 1, "yalnız geçmişsiz öğrenci silinmeli");
        assert!(
            summary.warnings.iter().any(|w| w.contains("GEÇMİŞLİ") && w.contains("geçmişi var")),
            "uyarı bulunamadı: {:?}",
            summary.warnings
        );

        let all = students::list(&pool).await.unwrap();
        assert!(all.iter().any(|s| s.first_name == "GEÇMİŞLİ"), "geçmişi olan öğrenci silinmemeli");
        assert!(!all.iter().any(|s| s.first_name == "SİLİNECEK"), "geçmişsiz öğrenci silinmeli");
        assert_eq!(all.len(), 17 + 1, "17 dosya öğrencisi + geçmişi olan öğrenci kalmalı");
    }
}
