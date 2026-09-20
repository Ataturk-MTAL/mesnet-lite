//! P6 (spec §10): tohumlu LCG ile rastgele komut dizileri. Reddedilenler de
//! sayılır. Her adımda:
//!   - önizleme HİÇBİR tabloyu değiştirmez;
//!   - aynı isteğin commit'i, önizlemenin sonucuyla (`Rejected` kodu ya da
//!     `Preview`/`Committed` etki özeti) aynıdır;
//!   - reddedilen istek hiçbir tabloyu değiştirmez;
//!   - her commit'ten sonra `verify_term` boştur ve tarih aralığı çakışması
//!     sorgusu 0 satır verir (tetikleyicilerden BAĞIMSIZ bir sorgudur);
//!   - hiçbir günde birden fazla öğretmen `department` (alan şefi) değildir:
//!     kural `revoke`/`correct` yoluyla da delinemez.
//!
//! Yeni bağımlılık yoktur (`proptest`/`fastrand` kapsam dışı, spec §11).

use std::collections::BTreeMap;
use std::time::Instant;

use chrono::{Duration, NaiveDate};
use sqlx::SqlitePool;

use super::change_service::{execute_change, ChangeMode, ChangeOutcome};
use super::change_service_test_support::*;
use crate::db::projection;
use crate::domain::history::decide::{ChangeCommand, ChangeRequest, TransferTarget};
use crate::domain::models::ChiefType;
use crate::domain::scheduling::Slot;

const SEQUENCES: u64 = 50;
const STEPS_PER_SEQUENCE: usize = 30;
/// Yürürlük tarihleri 25 Ekim ile 20 Aralık arasında seçilir: bir kısmı
/// önceki aya (reddedilir), bir kısmı ilerideki aylara düşer.
const DATE_SPAN_DAYS: u64 = 57;
const NONE_DATE_PERCENT: u64 = 5;
/// Sözleşmeye uygun (gerçek) önceki durumu kullanma olasılığı; kalanı
/// bilinçli olarak yanlış `from` verir ve `FactNotTrueAtDate` üretir.
const TRUTHFUL_FROM_PERCENT: u64 = 80;
/// Yük komutlarında şeflik türü olasılıkları. İki öğretmenle `Department`
/// sık denendiği için hem geçişler hem `ChiefAlreadyAssigned` redleri oluşur;
/// kalan pay `None`'dur ve şefliği bırakma yolunu da açık tutar.
const DEPARTMENT_PERCENT: u64 = 35;
const WORKSHOP_LAB_PERCENT: u64 = 15;

/// Knuth MMIX doğrusal eşlik üreteci: tohumlu, bağımsız, tekrarlanabilir.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn percent(&mut self, chance: u64) -> bool {
        self.next() % 100 < chance
    }

    fn pick(&mut self, items: &[i64]) -> i64 {
        items[self.below(items.len())]
    }
}

/// Bir dizinin okunan anlık durumu: hangi kimlikler var, öğrenciler nerede.
struct Pools {
    students: Vec<i64>,
    companies: Vec<i64>,
    teachers: Vec<i64>,
    placements: BTreeMap<i64, i64>,
    /// Geri alınabilir (geri alınmamış, geri alma/düzeltme olmayan) kümeler.
    revocable: Vec<i64>,
    counter: u64,
}

async fn refresh(pool: &SqlitePool, pools: &mut Pools) {
    pools.students = sqlx::query_scalar("SELECT id FROM students ORDER BY id").fetch_all(pool).await.unwrap();
    let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT student_id, company_id FROM student_placements WHERE term = ?1 AND valid_to IS NULL")
        .bind(TERM)
        .fetch_all(pool)
        .await
        .unwrap();
    pools.placements = rows.into_iter().collect();
}

fn random_date(rng: &mut Lcg) -> Option<NaiveDate> {
    if rng.percent(NONE_DATE_PERCENT) {
        return None;
    }
    Some(ymd(2026, 10, 25) + Duration::days(rng.below(DATE_SPAN_DAYS as usize) as i64))
}

/// `from` olarak gerçek işletme (çoğunlukla) ya da rastgele bir işletme.
fn from_company(rng: &mut Lcg, pools: &Pools, student_id: i64) -> i64 {
    match pools.placements.get(&student_id) {
        Some(actual) if rng.percent(TRUTHFUL_FROM_PERCENT) => *actual,
        _ => rng.pick(&pools.companies),
    }
}

fn student_command(rng: &mut Lcg, pools: &mut Pools) -> ChangeCommand {
    let student_id = rng.pick(&pools.students);
    let from = from_company(rng, pools, student_id);
    pools.counter += 1;
    match rng.below(6) {
        0 | 1 => ChangeCommand::TransferStudent { student_id, from_company_id: from, to: TransferTarget::Existing { company_id: rng.pick(&pools.companies) } },
        2 => {
            let company = new_company(&format!("P6 İşletme {}", pools.counter), Some(1.0 + rng.below(8) as f64));
            ChangeCommand::TransferStudent { student_id, from_company_id: from, to: TransferTarget::New { company } }
        }
        3 => ChangeCommand::StudentLeaves { student_id, from_company_id: from },
        4 => ChangeCommand::PlaceStudent { student_id, company_id: rng.pick(&pools.companies) },
        _ => {
            let company_id = if rng.percent(50) { Some(rng.pick(&pools.companies)) } else { None };
            ChangeCommand::CreateStudent { student: student_input(&format!("P6-{}", pools.counter)), company_id }
        }
    }
}

fn company_command(rng: &mut Lcg, pools: &Pools) -> ChangeCommand {
    let company_id = rng.pick(&pools.companies);
    match rng.below(4) {
        0 | 1 => {
            let mut row = hours_row(company_id, rng.below(13) as i64);
            row.is_honorary = rng.percent(10);
            row.is_locked = rng.percent(20);
            ChangeCommand::SetCompanyHours { rows: vec![row] }
        }
        2 => {
            let mut row = coordinator_row(company_id, rng.pick(&pools.teachers));
            row.visit_day = 1 + rng.below(5) as i64;
            row.visit_hour = 1 + rng.below(8) as i64;
            ChangeCommand::AssignCoordinators { rows: vec![row] }
        }
        _ if rng.percent(20) => ChangeCommand::ClearCoordination,
        _ => ChangeCommand::EndCoordination { company_id },
    }
}

fn random_chief_type(rng: &mut Lcg) -> ChiefType {
    match rng.next() % 100 {
        roll if roll < DEPARTMENT_PERCENT => ChiefType::Department,
        roll if roll < DEPARTMENT_PERCENT + WORKSHOP_LAB_PERCENT => ChiefType::WorkshopLab,
        _ => ChiefType::None,
    }
}

fn teacher_command(rng: &mut Lcg, pools: &Pools) -> ChangeCommand {
    let teacher_id = rng.pick(&pools.teachers);
    if rng.percent(50) {
        let mut load = standard_load();
        load.other_extra_hours = rng.below(25) as i64;
        load.chief_type = random_chief_type(rng);
        return ChangeCommand::SetTeacherLoad { teacher_id, load };
    }
    let slots = (0..rng.below(7)).map(|_| Slot::new(1 + rng.below(5) as i64, 1 + rng.below(8) as i64)).collect();
    ChangeCommand::SetTeacherSchedule { teacher_id, slots }
}

/// Geri alma ve düzeltme: çoğunlukla geri alınabilir bir küme hedeflenir;
/// bazen (bilinçli) çoktan geri alınmış olabilecek rastgele bir küme değil,
/// var olmayan bir kimlik verilir (`InvalidRequest`).
fn history_command(rng: &mut Lcg, pools: &Pools, student_id: i64) -> ChangeCommand {
    let target = if pools.revocable.is_empty() || rng.percent(10) { 9_999 } else { rng.pick(&pools.revocable) };
    if rng.percent(60) {
        return ChangeCommand::Revoke { change_set_id: target };
    }
    let replacement = ChangeCommand::StudentLeaves { student_id, from_company_id: rng.pick(&pools.companies) };
    ChangeCommand::Correct { change_set_id: target, replacement: Box::new(replacement) }
}

fn random_command(rng: &mut Lcg, pools: &mut Pools) -> ChangeCommand {
    match rng.below(10) {
        0..=3 => student_command(rng, pools),
        4..=6 => company_command(rng, pools),
        7 => teacher_command(rng, pools),
        8 => {
            let student_id = rng.pick(&pools.students);
            history_command(rng, pools, student_id)
        }
        _ if rng.percent(30) => ChangeCommand::DeleteStudent { student_id: rng.pick(&pools.students) },
        _ => company_command(rng, pools),
    }
}

/// Bir dizinin sahnesi: `world()` + üçüncü işletme, ikinci öğretmen, B'de iki öğrenci.
async fn sequence_world() -> (World, Pools) {
    let w = world().await;
    let today = planning_today();
    let company_c = add_company(&w.pool, "İşletme C", 1.0).await;
    let teacher_two = add_teacher(&w.pool, "Ela", today).await;
    for name in ["Emre", "Fatma"] {
        add_student(&w.pool, name, Some(w.company_b), today).await;
    }
    let pools = Pools {
        students: Vec::new(),
        companies: vec![w.company_a, w.company_b, company_c],
        teachers: vec![w.teacher, teacher_two],
        placements: BTreeMap::new(),
        revocable: Vec::new(),
        counter: 0,
    };
    (w, pools)
}

/// Aralık çakışması sorgusu: beş projeksiyonda, aynı özne ve dönemde
/// örtüşen iki satırın sayısı. Migration'daki tetikleyicilerden bağımsızdır.
async fn overlap_count(pool: &SqlitePool) -> i64 {
    let tables = [
        ("student_placements", "student_id"),
        ("company_hour_periods", "company_id"),
        ("coordination_periods", "company_id"),
        ("teacher_load_periods", "teacher_id"),
        ("teacher_schedule_periods", "teacher_id"),
    ];
    let mut total = 0;
    for (table, subject) in tables {
        let sql = format!(
            "SELECT COUNT(*) FROM {table} a JOIN {table} b
               ON a.{subject} = b.{subject} AND a.term = b.term AND a.id < b.id
              AND a.valid_from < COALESCE(b.valid_to, '9999-12-31')
              AND b.valid_from < COALESCE(a.valid_to, '9999-12-31')"
        );
        let count: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await.unwrap();
        total += count;
    }
    total
}

/// Alan şefliği (`department`) için tetikleyicilerden bağımsız sorgu: iki
/// FARKLI öğretmenin aynı dönemde örtüşen `department` aralığı sayısı.
/// (`overlap_count` aynı öğretmenin satırlarına bakar; bu, öğretmenler arası
/// kuralı denetler.)
async fn department_overlap_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM teacher_load_periods a JOIN teacher_load_periods b
           ON a.term = b.term AND a.teacher_id < b.teacher_id
          AND a.chief_type = 'department' AND b.chief_type = 'department'
          AND a.valid_from < COALESCE(b.valid_to, '9999-12-31')
          AND b.valid_from < COALESCE(a.valid_to, '9999-12-31')",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Sorgunun boş dönmesi, hiç `department` satırı yokken de doğru olurdu;
/// bu yüzden dizinin bir noktasında gerçekten satır görüldüğü de sayılır.
async fn department_row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM teacher_load_periods WHERE chief_type = 'department'").fetch_one(pool).await.unwrap()
}

#[derive(Default)]
struct Tally {
    committed: usize,
    /// Commit edilen, `department` yapan `setTeacherLoad` sayısı.
    committed_department_loads: usize,
    /// Herhangi bir commit'ten sonra projeksiyonda görülen en fazla `department` satırı.
    max_department_rows: i64,
    rejected: BTreeMap<String, usize>,
    /// Commit edilen komutların türü (`ChangeCommand`'ın varyant adı).
    committed_kinds: BTreeMap<String, usize>,
}

/// `ChangeCommand`'ın varyant adı (`Debug` çıktısının ilk sözcüğü).
fn variant_name(command: &ChangeCommand) -> String {
    format!("{command:?}").split(|c: char| !c.is_alphanumeric()).next().unwrap_or_default().to_string()
}

/// Bir adım: önizle, commit et, değişmezleri denetle.
async fn run_step(w: &World, req: ChangeRequest, seed: u64, step: usize, tally: &mut Tally) -> ChangeOutcome {
    let context = format!("dizi {seed}, adım {step}, istek {req:?}");
    let today = november_today();
    let before = table_counts(&w.pool).await;

    let previewed = execute_change(&w.pool, req.clone(), ChangeMode::Preview, today).await.unwrap_or_else(|e| panic!("önizleme AppError verdi ({e}) — {context}"));
    assert_eq!(table_counts(&w.pool).await, before, "önizleme bir şey yazdı — {context}");

    let ChangeOutcome::Preview { high_water, .. } = &previewed else {
        let committed = execute_change(&w.pool, req, ChangeMode::Commit { expected_high_water: None }, today).await.unwrap_or_else(|e| panic!("AppError ({e}) — {context}"));
        assert_eq!(committed, previewed, "reddedilen önizleme ile commit aynı sonucu vermeli — {context}");
        assert_eq!(table_counts(&w.pool).await, before, "reddedilen istek bir şey yazdı — {context}");
        let ChangeOutcome::Rejected { code, .. } = &previewed else { panic!("Rejected beklenirdi: {previewed:?} — {context}") };
        *tally.rejected.entry(format!("{code:?}")).or_default() += 1;
        return previewed;
    };

    let committed = execute_change(&w.pool, req, ChangeMode::Commit { expected_high_water: Some(*high_water) }, today)
        .await
        .unwrap_or_else(|e| panic!("commit AppError verdi ({e}) — {context}"));
    let (ChangeOutcome::Preview { impact: previewed_impact, .. }, ChangeOutcome::Committed { impact, .. }) = (&previewed, &committed) else {
        panic!("önizleme Preview ise commit Committed olmalı: {committed:?} — {context}");
    };
    assert_eq!(impact, previewed_impact, "önizlenen etki ile kaydedilen etki aynı olmalı — {context}");
    tally.committed += 1;
    committed
}

async fn assert_projection_is_consistent(pool: &SqlitePool, seed: u64, step: usize) {
    let mut conn = pool.acquire().await.unwrap();
    let drifts = projection::verify_term(&mut conn, TERM).await.unwrap();
    assert!(drifts.is_empty(), "dizi {seed}, adım {step}: projeksiyon günlükten saptı: {:?}", drifts.iter().map(|d| (&d.table, d.subject_id, &d.detail)).collect::<Vec<_>>());
    drop(conn);
    assert_eq!(overlap_count(pool).await, 0, "dizi {seed}, adım {step}: örtüşen tarih aralığı var");
}

/// Olay günlüğü dışındaki tek yazma kanalı (`RowAction`) commit ile birlikte
/// uygulanmış olmalı: kaydedilen `deleteStudent` öğrenci satırını kaldırır.
async fn assert_row_actions_were_applied(pool: &SqlitePool, command: &ChangeCommand, outcome: &ChangeOutcome, seed: u64, step: usize) {
    let (ChangeCommand::DeleteStudent { student_id }, ChangeOutcome::Committed { .. }) = (command, outcome) else { return };
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM students WHERE id = ?1").bind(student_id).fetch_one(pool).await.unwrap();
    assert_eq!(remaining, 0, "dizi {seed}, adım {step}: silinen öğrencinin satırı duruyor");
}

fn note_history_effect(command: &ChangeCommand, outcome: &ChangeOutcome, pools: &mut Pools) {
    let ChangeOutcome::Committed { change_set_id, .. } = outcome else { return };
    match command {
        ChangeCommand::Revoke { change_set_id: target } | ChangeCommand::Correct { change_set_id: target, .. } => {
            pools.revocable.retain(|id| id != target);
        }
        _ => pools.revocable.push(*change_set_id),
    }
}

fn makes_department(command: &ChangeCommand) -> bool {
    matches!(command, ChangeCommand::SetTeacherLoad { load, .. } if load.chief_type == ChiefType::Department)
}

async fn assert_single_department_per_day(pool: &SqlitePool, seed: u64, step: usize, tally: &mut Tally) {
    assert_eq!(department_overlap_count(pool).await, 0, "dizi {seed}, adım {step}: aynı günde birden fazla alan şefi var");
    tally.max_department_rows = tally.max_department_rows.max(department_row_count(pool).await);
}

async fn run_sequence(seed: u64, tally: &mut Tally) {
    let (w, mut pools) = sequence_world().await;
    let mut rng = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(12345));

    for step in 0..STEPS_PER_SEQUENCE {
        refresh(&w.pool, &mut pools).await;
        let command = random_command(&mut rng, &mut pools);
        let req = request(random_date(&mut rng), command.clone());
        let outcome = run_step(&w, req, seed, step, tally).await;
        if matches!(outcome, ChangeOutcome::Committed { .. }) {
            *tally.committed_kinds.entry(variant_name(&command)).or_default() += 1;
            tally.committed_department_loads += usize::from(makes_department(&command));
        }
        note_history_effect(&command, &outcome, &mut pools);
        assert_row_actions_were_applied(&w.pool, &command, &outcome, seed, step).await;
        assert_projection_is_consistent(&w.pool, seed, step).await;
        assert_single_department_per_day(&w.pool, seed, step, tally).await;
    }
}

#[tokio::test]
async fn p6_random_command_sequences_keep_the_projection_consistent() {
    let started = Instant::now();
    let mut tally = Tally::default();

    for seed in 0..SEQUENCES {
        run_sequence(seed, &mut tally).await;
    }

    let total = (SEQUENCES as usize) * STEPS_PER_SEQUENCE;
    let rejected: usize = tally.rejected.values().sum();
    println!("P6: {total} komut, {} commit {:?}, {rejected} red {:?}, süre {:?}", tally.committed, tally.committed_kinds, tally.rejected, started.elapsed());
    assert_eq!(tally.committed + rejected, total, "her komut ya commit ya red olmalı");
    // Üreteç anlamlı olmalı: hep reddeden bir dizi projeksiyonu sınamaz,
    // hiç reddetmeyen bir dizi reddetme yollarını.
    assert!(tally.committed * 10 >= total * 2, "commit oranı %20'nin altında: {}/{total}", tally.committed);
    assert!(rejected * 10 >= total, "red oranı %10'un altında: {rejected}/{total}");
    // Zor yollar gerçekten koşmuş olmalı: geri alma, düzeltme, yerinde
    // işletme oluşturma, zincir etkisi doğuran nakil ve saat.
    // Alan şefliği kuralı gerçekten sınanmış olmalı: `department` commit
    // edilmiş, projeksiyonda görülmüş ve en az bir kez reddedilmiş olmalı
    // (aksi hâlde "çakışma yok" değişmezi boş yere doğru olurdu).
    assert!(tally.committed_department_loads > 0, "hiç department yükü commit edilmedi: {:?}", tally.committed_kinds);
    assert!(tally.max_department_rows > 0, "projeksiyonda hiç department satırı görülmedi");
    assert!(tally.rejected.get("ChiefAlreadyAssigned").copied().unwrap_or(0) > 0, "kural hiç tetiklenmedi: {:?}", tally.rejected);
    for kind in ["Revoke", "Correct", "TransferStudent", "CreateStudent", "SetCompanyHours", "AssignCoordinators", "SetTeacherLoad"] {
        assert!(tally.committed_kinds.get(kind).copied().unwrap_or(0) > 0, "hiç commit edilmemiş komut türü: {kind} ({:?})", tally.committed_kinds);
    }
}
