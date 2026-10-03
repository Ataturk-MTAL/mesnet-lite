//! Amaç fonksiyonunun öncelikleri: bölge bütünlüğü, eşitlik eşiği, denge.

use super::*;

fn grouped(id: i64, key: &str, day: i64, teacher_id: Option<i64>) -> CompanyInput {
    CompanyInput {
        group_key: Some(key.into()),
        group_label: format!("Bölge {key}"),
        workplace_days: days(&[day]),
        current: teacher_id.and_then(|t| current(t, day, 9, PlacementSource::Manual)),
        ..company(id)
    }
}

fn two_day_teacher(id: i64, branch: &str) -> TeacherInput {
    TeacherInput {
        branches: vec![branch.into()],
        free_slots: free_on(&[1, 2], 9..13),
        ..teacher(id)
    }
}

/// T5: aynı bölgedeki işletmeye sahip öğretmen, dalı uymasa da seçilir.
#[test]
fn t5_group_cohesion_beats_branch_match() {
    let newcomer = CompanyInput {
        branches: vec!["Elektronik".into()],
        ..grouped(3, "g1", 2, None)
    };
    let companies = vec![
        grouped(1, "g1", 1, Some(1)),
        grouped(2, "g2", 1, Some(2)),
        newcomer,
    ];
    let teachers = vec![
        two_day_teacher(1, "Elektrik"),
        two_day_teacher(2, "Elektronik"),
    ];
    let proposal = Scenario::new(companies, teachers).run();

    assert_eq!(teacher_of(&proposal, 3), Some(1));
}

/// T5 karşıtı: bölge yoksa dal eşleşmesi eşitlik bozucu olarak çalışır.
#[test]
fn t5_without_groups_branch_decides() {
    let strip = |mut c: CompanyInput| {
        c.group_key = None;
        c
    };
    let newcomer = CompanyInput {
        branches: vec!["Elektronik".into()],
        ..grouped(3, "g1", 2, None)
    };
    let companies = vec![
        strip(grouped(1, "g1", 1, Some(1))),
        strip(grouped(2, "g2", 1, Some(2))),
        strip(newcomer),
    ];
    let teachers = vec![
        two_day_teacher(1, "Elektrik"),
        two_day_teacher(2, "Elektronik"),
    ];
    let proposal = Scenario::new(companies, teachers).run();

    assert_eq!(teacher_of(&proposal, 3), Some(2));
}

/// T6: dört eşit işletme, iki eşit öğretmen → 2 + 2, kesin dağılım.
#[test]
fn t6_equal_companies_split_two_and_two() {
    let teachers = vec![
        two_day_teacher(1, "Elektrik"),
        two_day_teacher(2, "Elektrik"),
    ];
    let companies = (1..=4).map(company).collect();
    let proposal = Scenario::new(companies, teachers).run();

    assert_eq!(teacher_of(&proposal, 1), Some(1));
    assert_eq!(teacher_of(&proposal, 2), Some(2));
    assert_eq!(teacher_of(&proposal, 3), Some(1));
    assert_eq!(teacher_of(&proposal, 4), Some(2));
}

/// Eşitlik eşiği: Ayşe 12 saatle "A" bölgesini tutuyor, Mehmet 4 saatle "B"
/// bölgesini. Yeni "A" işletmesi için eşik 4 iken saat farkı (16-4) eşiği
/// aşacağından az saatli Mehmet'e gider; eşik 20 iken bölge önce gelir.
fn gap_scenario(gap: i64) -> AllocationProposal {
    let with_thursday = |teacher_id: i64| {
        let mut teacher = two_day_teacher(teacher_id, "Elektrik");
        teacher.free_slots.extend(free(4, 9..13));
        teacher
    };
    let mut ayse = with_thursday(1);
    ayse.name = "Ayşe".into();
    ayse.free_slots.extend(free(3, 9..13));
    let mut mehmet = with_thursday(2);
    mehmet.name = "Mehmet".into();
    let companies = vec![
        grouped(1, "A", 1, Some(1)),
        grouped(2, "A", 2, Some(1)),
        grouped(3, "A", 3, Some(1)),
        grouped(4, "B", 1, Some(2)),
        grouped(5, "A", 4, None),
    ];
    Scenario::new(companies, vec![ayse, mehmet]).gap(gap).run()
}

#[test]
fn gap_threshold_4_sends_the_new_company_to_the_lighter_teacher() {
    assert_eq!(teacher_of(&gap_scenario(4), 5), Some(2));
}

#[test]
fn gap_threshold_20_keeps_the_new_company_in_its_group() {
    assert_eq!(teacher_of(&gap_scenario(20), 5), Some(1));
}

/// T16: bir öğretmen iki bölgeye yayılırsa rapor edilir.
#[test]
fn t16_group_split_is_reported() {
    let teacher = TeacherInput {
        free_slots: free_on(&[1, 2], 9..17),
        ..teacher(1)
    };
    let companies = vec![grouped(1, "A", 1, None), grouped(2, "B", 2, None)];
    let proposal = Scenario::new(companies, vec![teacher]).run();

    assert_eq!(proposal.group_splits.len(), 1);
    let split = &proposal.group_splits[0];
    assert_eq!(split.teacher_id, 1);
    let keys: Vec<&str> = split.groups.iter().map(|g| g.group_key.as_str()).collect();
    assert_eq!(keys, vec!["A", "B"]);
    assert_eq!(split.groups[0].company_names, vec!["İşletme 1".to_string()]);
    assert_eq!(proposal.teacher_loads[0].distinct_groups, 2);
}

/// Aynı bölgenin işletmeleri tek öğretmende toplanırsa bölünme raporlanmaz.
#[test]
fn companies_of_one_group_on_one_teacher_are_not_a_split() {
    let teacher = TeacherInput {
        free_slots: free_on(&[1, 2], 9..17),
        ..teacher(1)
    };
    let companies = vec![grouped(1, "A", 1, None), grouped(2, "A", 2, None)];
    let proposal = Scenario::new(companies, vec![teacher]).run();

    assert!(proposal.group_splits.is_empty());
}

/// Saat toplamları eşitken işletme sayısı dengesi karar verir (Σ işletme²):
/// 1 numaralı öğretmende iki, 2 numaralıda tek sabit işletme var (ikisi de 4
/// saat); yeni işletme tekin tarafa, yani 2 numaraya gider. Düşük sıra
/// eşitlik bozucu olsaydı 1 numara seçilirdi.
#[test]
fn equal_hours_are_broken_by_company_count_balance() {
    let fixed = |id: i64, teacher_id: i64, hour: i64, hours: i64| CompanyInput {
        awarded_hours: hours,
        max_hours: hours,
        workplace_days: days(&[1]),
        current: current(teacher_id, 1, hour, PlacementSource::Manual),
        ..company(id)
    };
    let newcomer = CompanyInput {
        workplace_days: days(&[2]),
        ..company(4)
    };
    let teachers = vec![
        two_day_teacher(1, "Elektrik"),
        two_day_teacher(2, "Elektrik"),
    ];
    let companies = vec![
        fixed(1, 1, 9, 2),
        fixed(2, 1, 11, 2),
        fixed(3, 2, 9, 4),
        newcomer,
    ];
    let proposal = Scenario::new(companies, teachers).run();

    assert_eq!(teacher_of(&proposal, 4), Some(2));
}
