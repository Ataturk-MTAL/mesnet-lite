//! Saat kararları: kısaltma, havuz, kapasite, günlük sınır, fahri.

use super::*;

fn one_teacher(cells: std::ops::Range<i64>) -> TeacherInput {
    TeacherInput {
        free_slots: free(1, cells),
        ..teacher(1)
    }
}

/// T1: 4 saatlik işletme 3 ardışık hücreye 3 saat olarak sığar.
#[test]
fn t1_hours_shrink_to_the_longest_block_that_fits() {
    let proposal = Scenario::new(vec![company(1)], vec![one_teacher(9..12)]).run();

    assert_eq!(hours_of(&proposal, 1), Some(3));
    assert!(proposal.unassigned.is_empty());
    let change = change_of(&proposal, 1).expect("saat değişikliği raporlanmalı");
    assert_eq!((change.old_hours, change.new_hours), (4, 3));
    assert_eq!(change.reason_code, HourChangeReason::NoConsecutiveCells);
}

/// T2: tam saat, kısaltmaya yeğdir; 4 hücreli öğretmen seçilir.
#[test]
fn t2_full_hours_beat_shrinking_on_another_teacher() {
    let short = one_teacher(9..11);
    let long = TeacherInput {
        free_slots: free(1, 9..13),
        ..teacher(2)
    };
    let proposal = Scenario::new(vec![company(1)], vec![short, long]).run();

    assert_eq!(final_of(&proposal, 1).map(|f| (f.0, f.3)), Some((2, 4)));
    assert!(proposal.hour_changes.is_empty());
}

/// T3: iki işletmeyi yerleştirmek (2 yerleşen, 4 saat), tek işletmeyi tam
/// saatle bırakmaktan (1 yerleşen, 4 saat) üstündür; A 4 → 2'ye iner.
#[test]
fn t3_shrink_via_ejection_places_both_companies() {
    let a = company(1);
    let b = CompanyInput {
        awarded_hours: 2,
        max_hours: 2,
        ..company(2)
    };
    let proposal = Scenario::new(vec![a, b], vec![one_teacher(9..13)]).run();

    assert_eq!(hours_of(&proposal, 1), Some(2));
    assert_eq!(hours_of(&proposal, 2), Some(2));
    let change = change_of(&proposal, 1).expect("A'nın saati düşmeli");
    assert_eq!((change.old_hours, change.new_hours), (4, 2));
    assert_eq!(
        change.reason_code,
        HourChangeReason::MadeRoomFor { company_id: 2 }
    );
}

/// T4: Redistribute'ta önerilmiş atama, tam saat alabileceği güne taşınır.
#[test]
fn t4_redistribute_moves_a_proposal_company_to_regain_hours() {
    let a = CompanyInput {
        awarded_hours: 2,
        current: current(1, 2, 9, PlacementSource::Proposal),
        ..company(1)
    };
    let teacher = TeacherInput {
        free_slots: free(1, 9..13).into_iter().chain(free(2, 9..11)).collect(),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![a], vec![teacher]).redistribute().run();

    assert_eq!(final_of(&proposal, 1), Some((1, 1, 9, 4)));
    let change = change_of(&proposal, 1).expect("saat 2 → 4");
    assert_eq!((change.old_hours, change.new_hours), (2, 4));
    assert_eq!(change.reason_code, HourChangeReason::Granted);
    assert_eq!(
        proposal.assignments[0]
            .previous
            .as_ref()
            .map(|p| p.visit_day),
        Some(2)
    );
}

/// T9: tek öğretmen, tek gün, üç işletme: hepsi yerleşir, hiçbiri 0'a düşmez.
#[test]
fn t9_three_companies_share_one_day_without_going_honorary() {
    let big = |id| CompanyInput {
        awarded_hours: 8,
        max_hours: 8,
        workplace_days: days(&[1]),
        ..company(id)
    };
    let proposal = Scenario::new(vec![big(1), big(2), big(3)], vec![one_teacher(9..17)]).run();

    assert_eq!(proposal.placed_count, 3);
    assert_eq!(proposal.total_hours, 8);
    for id in 1..=3 {
        assert!(
            hours_of(&proposal, id).unwrap() > 0,
            "işletme {id} 0'a düşmemeli"
        );
    }
}

/// İki tavan-8 işletme; `awarded` mevcut saatleri verir (havuz taşması
/// `eski toplam > havuz` ile belirlendiği için testler bunu bilerek seçer).
fn pool_scenario(awarded: [i64; 2], teachers: Vec<TeacherInput>) -> Scenario {
    let big = |id: i64| CompanyInput {
        awarded_hours: awarded[(id - 1) as usize],
        max_hours: 8,
        workplace_days: days(&[1, 2]),
        ..company(id)
    };
    Scenario::new(vec![big(1), big(2)], teachers)
}

/// Pazartesi + Salı tam gün boş: günlük 8 saat sınırı tek günde 10 saate
/// izin vermediği için havuz testleri iki güne yayılır.
fn mon_tue_teacher(id: i64) -> TeacherInput {
    TeacherInput {
        free_slots: free_on(&[1, 2], 9..17),
        ..teacher(id)
    }
}

/// T10: havuz 10, iki tavan-8 işletme, iki öğretmen → 5 + 5.
#[test]
fn t10_pool_is_shared_equally_between_two_teachers() {
    let proposal = pool_scenario([6, 4], vec![mon_tue_teacher(1), mon_tue_teacher(2)])
        .pool(10)
        .run();

    assert_eq!(hours_of(&proposal, 1), Some(5));
    assert_eq!(hours_of(&proposal, 2), Some(5));
    assert_eq!(proposal.pool_remaining, Some(0));
    assert_eq!(
        change_of(&proposal, 1).map(|c| c.reason_code),
        Some(HourChangeReason::PoolExhausted)
    );
}

/// T10 varyantı: tek öğretmende de havuz tam kullanılır, ikisi de yerleşir.
#[test]
fn t10_single_teacher_still_spends_the_whole_pool() {
    let proposal = pool_scenario([6, 4], vec![mon_tue_teacher(1)])
        .pool(10)
        .run();

    assert_eq!(proposal.placed_count, 2);
    assert_eq!(proposal.total_hours, 10);
}

/// Toplam saat değişmiyorsa mevcut dağılım korunur: hareket maliyeti (churn)
/// yalnız başka hiçbir terim ayırt etmediğinde devreye girer.
#[test]
fn single_teacher_keeps_the_current_hour_split_when_the_total_is_unchanged() {
    let proposal = pool_scenario([6, 4], vec![mon_tue_teacher(1)])
        .pool(10)
        .run();

    assert_eq!(hours_of(&proposal, 1), Some(6));
    assert_eq!(hours_of(&proposal, 2), Some(4));
    assert!(proposal.hour_changes.is_empty());
}

/// T10 varyantı: havuz tanımsız (`<= 0`) ise kısıt yok; tavanlar uygulanır.
#[test]
fn t10_undefined_pool_does_not_constrain_hours() {
    let proposal = pool_scenario([4, 4], vec![mon_tue_teacher(1), mon_tue_teacher(2)]).run();

    assert_eq!(proposal.total_hours, 16);
    assert_eq!(proposal.pool_remaining, None);
}

/// T10 varyantı: havuz zaten aşılmışsa toplam ARTMAZ ama mevcut toplama kadar
/// yeniden dağıtılabilir (bütçe = max(havuz, eski toplam)). Eski toplam 14,
/// havuz 10: 8 + 6 → 7 + 7; kısılan işletmenin nedeni "havuz zaten aşılmış".
#[test]
fn t10_overrun_pool_redistributes_without_growing_the_total() {
    let proposal = pool_scenario([8, 6], vec![mon_tue_teacher(1), mon_tue_teacher(2)])
        .pool(10)
        .run();

    assert!(proposal.total_hours <= 14, "toplam artmamalı");
    assert_eq!(hours_of(&proposal, 1), Some(7));
    assert_eq!(hours_of(&proposal, 2), Some(7));
    assert_eq!(
        change_of(&proposal, 1).map(|c| c.reason_code),
        Some(HourChangeReason::PoolAlreadyOverrun)
    );
    assert!(!proposal.warnings.is_empty(), "taşma uyarısı verilmeli");
}

/// Havuz aşılmışken dengeleme gerekmiyorsa mevcut saatler sıfırlanmaz.
#[test]
fn t10_overrun_pool_keeps_current_hours_when_nothing_improves() {
    let proposal = pool_scenario([8, 8], vec![mon_tue_teacher(1), mon_tue_teacher(2)])
        .pool(10)
        .run();

    assert_eq!(proposal.total_hours, 16);
    assert!(proposal.hour_changes.is_empty());
}

/// T11: kapasite bitmişse işletme yerleşmesiz kalmaz; fahri (0 saat) olur.
#[test]
fn t11_no_capacity_makes_the_company_honorary_not_unplaced() {
    let teacher = TeacherInput {
        capacity: 0,
        ..one_teacher(9..13)
    };
    let proposal = Scenario::new(vec![company(1)], vec![teacher]).run();

    assert_eq!(hours_of(&proposal, 1), Some(0));
    assert!(proposal.unassigned.is_empty());
    assert_eq!(
        change_of(&proposal, 1).map(|c| c.reason_code),
        Some(HourChangeReason::TeacherCapacity)
    );
}

/// T12: günlük 8 saat sınırı (OÖKY MADDE 88) işletmeleri farklı güne yayar.
#[test]
fn t12_daily_cap_spreads_companies_across_days() {
    let five = |id| CompanyInput {
        awarded_hours: 5,
        max_hours: 5,
        ..company(id)
    };
    let teacher = TeacherInput {
        free_slots: free_on(&[1, 2], 9..17),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![five(1), five(2)], vec![teacher]).run();

    assert_eq!(hours_of(&proposal, 1), Some(5));
    assert_eq!(hours_of(&proposal, 2), Some(5));
    let day_1 = final_of(&proposal, 1).unwrap().1;
    let day_2 = final_of(&proposal, 2).unwrap().1;
    assert_ne!(day_1, day_2);
}

/// T17: taban yük (motor dışı atamalar) günlük sınırdan düşülür.
#[test]
fn t17_base_hours_on_a_day_reduce_what_fits() {
    let teacher = TeacherInput {
        base_assigned_hours: 6,
        base_hours_by_day: [(1, 6)].into_iter().collect(),
        ..one_teacher(9..13)
    };
    let proposal = Scenario::new(vec![company(1)], vec![teacher]).run();

    assert_eq!(hours_of(&proposal, 1), Some(2), "6 + 2 = 8 saat/gün");
    assert_eq!(
        change_of(&proposal, 1).map(|c| c.reason_code),
        Some(HourChangeReason::DailyCap)
    );
}

/// Motor-fahri son çaredir: hücre ve kapasite 8 + 0 'a izin verirken 7 + 1
/// tercih edilir (ürün kararı 2026-10-02).
#[test]
fn engine_prefers_seven_plus_one_over_eight_plus_zero() {
    let big = |id| CompanyInput {
        awarded_hours: 8,
        max_hours: 8,
        workplace_days: days(&[1]),
        ..company(id)
    };
    let teacher = TeacherInput {
        capacity: 8,
        free_slots: free(1, 9..18),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![big(1), big(2)], vec![teacher])
        .day_end(18)
        .run();

    let mut hours = vec![
        hours_of(&proposal, 1).unwrap(),
        hours_of(&proposal, 2).unwrap(),
    ];
    hours.sort_unstable();
    assert_eq!(hours, vec![1, 7]);
}

/// Kapasite gerçekten yetmediğinde (2 saat, 3 işletme) tam bir işletme fahri
/// olur; yerleşmesiz kalan olmaz.
#[test]
fn engine_honorary_appears_only_when_capacity_truly_runs_out() {
    let teacher = TeacherInput {
        capacity: 2,
        ..one_teacher(9..17)
    };
    let proposal = Scenario::new(vec![company(1), company(2), company(3)], vec![teacher]).run();

    let mut hours: Vec<i64> = (1..=3).map(|id| hours_of(&proposal, id).unwrap()).collect();
    hours.sort_unstable();
    assert_eq!(hours, vec![0, 1, 1]);
    assert!(proposal.unassigned.is_empty());
}

/// Kullanıcı-fahri (kilitsiz) işletme 0'a çakılı değildir: hücre, kapasite ve
/// havuz yettiğinde saat alır ve bu bir saat değişikliği olarak görünür.
#[test]
fn user_honorary_company_receives_hours_when_resources_allow() {
    let honorary = CompanyInput {
        is_honorary: true,
        awarded_hours: 0,
        max_hours: 4,
        ..company(1)
    };
    let proposal = Scenario::new(vec![honorary], vec![one_teacher(9..13)])
        .pool(10)
        .run();

    assert_eq!(hours_of(&proposal, 1), Some(4));
    let change = change_of(&proposal, 1).expect("0 → 4 raporlanmalı");
    assert_eq!((change.old_hours, change.new_hours), (0, 4));
}

/// Kilitli fahri işletmenin saati (0) değişmez.
#[test]
fn locked_honorary_company_stays_at_zero() {
    let honorary = CompanyInput {
        is_honorary: true,
        is_locked: true,
        awarded_hours: 0,
        max_hours: 4,
        ..company(1)
    };
    let proposal = Scenario::new(vec![honorary], vec![one_teacher(9..13)]).run();

    assert_eq!(hours_of(&proposal, 1), Some(0));
    assert!(proposal.hour_changes.is_empty());
}

/// Tavan, işletmenin mevcut saatinin altına inmişse neden `CeilingLowered`.
#[test]
fn lowered_ceiling_is_reported_as_such() {
    let capped = CompanyInput {
        awarded_hours: 6,
        max_hours: 3,
        ..company(1)
    };
    let proposal = Scenario::new(vec![capped], vec![one_teacher(9..17)]).run();

    assert_eq!(hours_of(&proposal, 1), Some(3));
    assert_eq!(
        change_of(&proposal, 1).map(|c| c.reason_code),
        Some(HourChangeReason::CeilingLowered)
    );
}

fn locked(id: i64, hours: i64) -> CompanyInput {
    CompanyInput {
        is_locked: true,
        awarded_hours: hours,
        max_hours: hours,
        ..company(id)
    }
}

/// Kilit saati sabitler: 4 saatlik kilitli işletme 3 hücreye 3 saat olarak
/// sığmaz; yerleşemeyen olur ve saati aynen kalır.
#[test]
fn locked_company_without_assignment_is_placed_with_exact_hours_or_not_at_all() {
    let proposal = Scenario::new(vec![locked(1, 4)], vec![one_teacher(9..12)]).run();

    assert_eq!(proposal.unassigned.len(), 1);
    assert!(proposal.hour_changes.is_empty());

    let proposal = Scenario::new(vec![locked(1, 4)], vec![one_teacher(9..13)]).run();
    assert_eq!(hours_of(&proposal, 1), Some(4));
}

/// Yerleşemeyen kilitli işletmenin saati yürürlükte kalır; havuzda ayrılır.
#[test]
fn unplaced_locked_company_reserves_its_hours_in_the_pool() {
    let stranded = CompanyInput {
        workplace_days: days(&[5]),
        ..locked(1, 4)
    };
    let other = CompanyInput {
        awarded_hours: 2,
        max_hours: 8,
        ..company(2)
    };
    let teacher = TeacherInput {
        free_slots: free_on(&[1, 2], 9..17),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![stranded, other], vec![teacher])
        .pool(10)
        .run();

    assert_eq!(hours_of(&proposal, 2), Some(6), "10 - 4 kilitli");
    assert!(change_of(&proposal, 1).is_none());
}

/// Yerleşemeyen oynak işletmenin saati kayıtta kalmasın: old → 0, `Unplaced`.
#[test]
fn unplaced_movable_company_returns_its_hours_to_the_pool() {
    let stranded = CompanyInput {
        workplace_days: days(&[5]),
        ..company(1)
    };
    let proposal = Scenario::new(vec![stranded], vec![one_teacher(9..13)]).run();

    let change = change_of(&proposal, 1).expect("saat 4 → 0 raporlanmalı");
    assert_eq!((change.old_hours, change.new_hours), (4, 0));
    assert_eq!(change.reason_code, HourChangeReason::Unplaced);
    assert_eq!(proposal.unassigned.len(), 1);
}

/// Saati zaten 0 olan yerleşemeyen işletme için gürültü üretilmez.
#[test]
fn unplaced_company_with_zero_hours_has_no_hour_change() {
    let stranded = CompanyInput {
        workplace_days: days(&[5]),
        awarded_hours: 0,
        ..company(1)
    };
    let proposal = Scenario::new(vec![stranded], vec![one_teacher(9..13)]).run();

    assert!(proposal.hour_changes.is_empty());
}
