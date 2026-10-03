//! Sabit olanlar, kipler, yerleşemeyenler ve belirlenimcilik.

use super::*;

/// İki öğretmen: 1 numara yalnız Salı, 2 numara Pazartesi + Salı boş.
/// 2 numaranın üstünde iki işletme (Pazartesi ve Salı 4'er saat) yüklüdür.
fn overloaded_pair(first: CompanyInput, second: CompanyInput) -> Scenario {
    let relief = TeacherInput {
        free_slots: free(2, 9..13),
        ..teacher(1)
    };
    let loaded = TeacherInput {
        free_slots: free_on(&[1, 2], 9..13),
        ..teacher(2)
    };
    Scenario::new(vec![first, second], vec![relief, loaded]).redistribute()
}

fn at_loaded(id: i64, day: i64, source: PlacementSource) -> CompanyInput {
    CompanyInput {
        current: current(2, day, 9, source),
        ..company(id)
    }
}

/// T7: kilitli atama Redistribute'ta yerinde kalır; kilitsiz öneri taşınır.
#[test]
fn t7_locked_company_stays_while_an_unlocked_proposal_moves() {
    let locked = CompanyInput {
        is_locked: true,
        ..at_loaded(1, 1, PlacementSource::Proposal)
    };
    let movable = at_loaded(2, 2, PlacementSource::Proposal);
    let proposal = overloaded_pair(locked, movable).run();

    assert_eq!(final_of(&proposal, 1), Some((2, 1, 9, 4)));
    assert!(proposal
        .kept
        .iter()
        .any(|k| k.company_id == 1 && k.is_locked));
    assert_eq!(final_of(&proposal, 2), Some((1, 2, 9, 4)));
    assert_eq!(proposal.assignments.len(), 1);
}

/// `source == Manual` atama (kilitsiz, zorlamasız) Redistribute'ta yerinde
/// kalır; elle yapılan karar geri alınmaz.
#[test]
fn manual_assignment_stays_in_redistribute() {
    let manual = at_loaded(1, 1, PlacementSource::Manual);
    let movable = at_loaded(2, 2, PlacementSource::Proposal);
    let proposal = overloaded_pair(manual, movable).run();

    assert_eq!(teacher_of(&proposal, 1), Some(2));
    assert_eq!(teacher_of(&proposal, 2), Some(1));
}

/// İkisi de `Proposal` ise yük dengesi için BİRİ taşınabilir.
#[test]
fn proposal_assignments_may_move_in_redistribute() {
    let first = at_loaded(1, 1, PlacementSource::Proposal);
    let second = at_loaded(2, 2, PlacementSource::Proposal);
    let proposal = overloaded_pair(first, second).run();

    assert_eq!(proposal.assignments.len(), 1);
    assert_eq!(proposal.assignments[0].teacher_id, 1);
    assert!(proposal.assignments[0].previous.is_some());
}

/// T15: zorlanmış atama, kaynağı öneri olsa bile Redistribute'ta sabittir.
#[test]
fn t15_forced_assignment_is_kept_in_redistribute() {
    let mut forced = at_loaded(2, 2, PlacementSource::Proposal);
    if let Some(placement) = forced.current.as_mut() {
        placement.is_forced = true;
    }
    let proposal = overloaded_pair(at_loaded(1, 1, PlacementSource::Manual), forced).run();

    assert!(proposal.assignments.is_empty());
    assert!(proposal
        .kept
        .iter()
        .any(|k| k.company_id == 2 && k.is_forced));
}

/// T8: FillGaps mevcut atamalara dokunmaz, yalnız yenileri yerleştirir.
#[test]
fn t8_fill_gaps_leaves_existing_assignments_untouched() {
    let existing = CompanyInput {
        current: current(2, 1, 9, PlacementSource::Proposal),
        ..company(1)
    };
    let teachers = vec![
        TeacherInput {
            free_slots: free_on(&[1, 2], 9..13),
            ..teacher(1)
        },
        TeacherInput {
            free_slots: free_on(&[1, 2], 9..13),
            ..teacher(2)
        },
    ];
    let proposal = Scenario::new(vec![existing, company(2)], teachers).run();

    assert_eq!(final_of(&proposal, 1), Some((2, 1, 9, 4)));
    assert_eq!(proposal.assignments.len(), 1);
    assert_eq!(proposal.assignments[0].company_id, 2);
    assert_eq!(proposal.assignments[0].teacher_id, 1, "yük dengesi");
    assert!(proposal.released.is_empty());
}

/// Taşımanın kazancı yoksa Redistribute'ta önerilmiş atama da yerinde kalır
/// (`churn` eşitlik bozucu) ve `kept` içinde görünür.
#[test]
fn redistribute_keeps_a_proposal_company_when_nothing_improves() {
    let stay = CompanyInput {
        current: current(1, 1, 9, PlacementSource::Proposal),
        ..company(1)
    };
    let teachers = vec![
        TeacherInput {
            free_slots: free(1, 9..13),
            ..teacher(1)
        },
        TeacherInput {
            free_slots: free(1, 9..13),
            ..teacher(2)
        },
    ];
    let proposal = Scenario::new(vec![stay], teachers).redistribute().run();

    assert!(proposal.assignments.is_empty());
    assert_eq!(final_of(&proposal, 1), Some((1, 1, 9, 4)));
    assert!(proposal.hour_changes.is_empty());
}

/// Redistribute'ta bir öneri yerleştirilemez hâle gelirse serbest bırakılır.
#[test]
fn redistribute_releases_a_proposal_company_that_no_longer_fits() {
    let stranded = CompanyInput {
        workplace_days: days(&[5]),
        current: current(1, 1, 9, PlacementSource::Proposal),
        ..company(1)
    };
    let teacher = TeacherInput {
        free_slots: free(1, 9..13),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![stranded], vec![teacher])
        .redistribute()
        .run();

    assert_eq!(proposal.released.len(), 1);
    assert_eq!(proposal.released[0].company_id, 1);
    assert_eq!(
        change_of(&proposal, 1).map(|c| (c.old_hours, c.new_hours, c.reason_code)),
        Some((4, 0, HourChangeReason::Unplaced))
    );
    assert_eq!(proposal.unassigned.len(), 1);
    assert!(proposal.unassigned[0].was_assigned);
}

/// Girdide olmayan öğretmene bağlı sabit atama sessizce yutulmaz.
#[test]
fn fixed_company_with_unknown_teacher_is_kept_with_a_warning() {
    let orphan = CompanyInput {
        current: current(99, 1, 9, PlacementSource::Manual),
        ..company(1)
    };
    let teacher = TeacherInput {
        free_slots: free(1, 9..13),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![orphan], vec![teacher]).run();

    assert_eq!(final_of(&proposal, 1), Some((99, 1, 9, 4)));
    assert!(!proposal.warnings.is_empty());
}

/// T13: yerleşemeyen her işletme kendi nedenini taşır.
#[test]
fn t13_unplaced_companies_carry_distinct_reasons() {
    let no_days = CompanyInput {
        workplace_days: days(&[]),
        ..company(1)
    };
    let friday_only = CompanyInput {
        workplace_days: days(&[5]),
        ..company(2)
    };
    let tuesday_only = CompanyInput {
        workplace_days: days(&[2]),
        ..company(3)
    };
    let busy = TeacherInput {
        free_slots: free(2, 9..10),
        base_used_slots: free(2, 9..10),
        ..teacher(2)
    };
    let open = TeacherInput {
        free_slots: free(1, 9..13),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![no_days, friday_only, tuesday_only], vec![open, busy]).run();

    let reason = |id: i64| {
        proposal
            .unassigned
            .iter()
            .find(|u| u.company_id == id)
            .unwrap_or_else(|| panic!("işletme {id} yerleşmemeliydi"))
    };
    assert_eq!(reason(1).reason_code, UnassignedReason::NoWorkplaceDays);
    assert_eq!(reason(2).reason_code, UnassignedReason::NoEligibleCell);
    assert!(reason(2).reason.contains("Cuma"), "mesaj günü adlandırmalı");
    assert_eq!(
        reason(3).reason_code,
        UnassignedReason::AllEligibleCellsOccupied
    );
    assert!(!reason(1).was_assigned);
}

#[test]
fn t13_no_teachers_leaves_every_company_unplaced() {
    let proposal = Scenario::new(vec![company(1)], vec![]).run();

    assert_eq!(proposal.unassigned.len(), 1);
    assert_eq!(
        proposal.unassigned[0].reason_code,
        UnassignedReason::NoTeachers
    );
    assert_eq!(proposal.placed_count, 0);
}

/// T14: girdi sırası sonucu etkilemez; çıktı bayt bayt aynıdır.
#[test]
fn t14_output_is_identical_for_any_input_order() {
    let companies: Vec<CompanyInput> = (1..=9)
        .map(|id| CompanyInput {
            awarded_hours: 2 + id % 4,
            max_hours: 2 + id % 4,
            workplace_days: days(&[1 + id % 3, 2 + id % 4]),
            group_key: (id % 3 != 0).then(|| format!("g{}", id % 3)),
            ..company(id)
        })
        .collect();
    let teachers: Vec<TeacherInput> = (1..=3)
        .map(|id| TeacherInput {
            free_slots: free_on(&[1, 2, 3, 4, 5], 9..(11 + id)),
            ..teacher(id)
        })
        .collect();

    let forward = Scenario::new(companies.clone(), teachers.clone());
    let mut reversed = Scenario::new(companies, teachers);
    reversed.companies.reverse();
    reversed.teachers.reverse();

    let a = serde_json::to_string(&forward.run()).unwrap();
    let b = serde_json::to_string(&reversed.run()).unwrap();
    assert_eq!(a, b);
    assert_eq!(a, serde_json::to_string(&forward.run()).unwrap(), "tekrar");
}

/// Ön yüzün okuduğu JSON anahtarları camelCase'dir.
#[test]
fn proposal_serializes_with_camel_case_keys() {
    let proposal = Scenario::new(vec![company(1)], vec![]).run();
    let json = serde_json::to_value(&proposal).unwrap();

    for key in [
        "hourChanges",
        "poolRemaining",
        "teacherLoads",
        "placedCount",
    ] {
        assert!(json.get(key).is_some(), "{key} yok");
    }
    let reason = serde_json::to_value(HourChangeReason::MadeRoomFor { company_id: 7 }).unwrap();
    assert_eq!(reason["kind"], "madeRoomFor");
    assert_eq!(reason["companyId"], 7);
}

/// Izgara kenarını aşan sabit blok, ızgara İÇİNDEKİ hücrelerini yine de ayırır:
/// 15–18 saatlik sabit atama 15 ve 16'yı tutar, B oraya konamaz.
#[test]
fn fixed_block_crossing_the_grid_edge_still_reserves_its_in_grid_cells() {
    let fixed = CompanyInput {
        current: current(1, 1, 15, PlacementSource::Manual),
        ..company(1)
    };
    let small = CompanyInput {
        awarded_hours: 2,
        max_hours: 2,
        workplace_days: days(&[1]),
        ..company(2)
    };
    let teacher = TeacherInput {
        free_slots: free(1, 15..17),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![fixed, small], vec![teacher]).run();

    assert!(proposal.unassigned.iter().any(|u| u.company_id == 2));
    assert!(proposal.assignments.is_empty());
}

/// Saçma büyük saat panik üretmez (taşma yok).
#[test]
fn absurd_hours_do_not_overflow() {
    let fixed = CompanyInput {
        awarded_hours: i64::MAX,
        current: current(1, 1, 9, PlacementSource::Manual),
        ..company(1)
    };
    let movable = CompanyInput {
        awarded_hours: i64::MAX,
        max_hours: i64::MAX,
        ..company(2)
    };
    let teacher = TeacherInput {
        free_slots: free(2, 9..17),
        ..teacher(1)
    };
    let proposal = Scenario::new(vec![fixed, movable], vec![teacher])
        .pool(10)
        .run_unchecked();

    assert_eq!(
        proposal.kept.len() + proposal.assignments.len() + proposal.unassigned.len(),
        2
    );
}
