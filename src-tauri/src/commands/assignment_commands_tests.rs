    use super::*;
    use crate::db::init_pool;
    use crate::db::teaching_load::TermBranchHoursInput;
    use crate::db::teaching_load_test_support::{seed_teacher, ymd, TERM};
    use crate::domain::models::ChiefType;
    use crate::error::AppError;
    use sqlx::SqlitePool;

    #[test]
    fn occupied_cells_by_teacher_expands_a_multi_hour_block() {
        // Öğretmen 1, 2. gün 3. saatten başlayıp 3 hücre kaplayan bir blokla dolu.
        let placements = [(1, 2, 3, 5, 100)];
        let occupied = occupied_cells_by_teacher(&placements);

        let teacher_cells = &occupied[&1];
        assert_eq!(teacher_cells.len(), 3);
        assert_eq!(teacher_cells["2-3"], 100);
        assert_eq!(teacher_cells["2-4"], 100);
        assert_eq!(teacher_cells["2-5"], 100);
    }

    /// Fahri ziyaret (başlangıç = bitiş) tek hücre kaplar.
    #[test]
    fn occupied_cells_by_teacher_handles_a_single_cell_block() {
        let placements = [(1, 1, 9, 9, 200)];
        let occupied = occupied_cells_by_teacher(&placements);

        assert_eq!(occupied[&1].len(), 1);
        assert_eq!(occupied[&1]["1-9"], 200);
    }

    #[test]
    fn occupied_cells_by_teacher_keeps_teachers_separate() {
        let placements = [(1, 1, 9, 10, 100), (2, 1, 9, 9, 200)];
        let occupied = occupied_cells_by_teacher(&placements);

        assert_eq!(occupied.len(), 2);
        assert_eq!(occupied[&1]["1-9"], 100);
        assert_eq!(occupied[&2]["1-9"], 200);
    }

    #[test]
    fn occupied_cells_by_teacher_is_empty_for_no_placements() {
        assert!(occupied_cells_by_teacher(&[]).is_empty());
    }

    /// Atama tahtasındaki işletme kartı ilçeyi taşımalı — İşletme Dağıtımı
    /// ekranı atanmamış işletmeleri ilçe bazlı gruplayacak (kaynak talep).
    #[tokio::test]
    async fn board_company_carries_the_district_field() {
        use crate::domain::models::NewCompany;

        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        companies::create(
            &pool,
            &NewCompany {
                name: "Test İşletme".into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "33130 Akdeniz/Mersin".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(5.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap();

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.companies.len(), 1);
        assert_eq!(board.companies[0].district, "Akdeniz");
    }

    /// Atama tahtasının saat aralığı da AYNI türetmeden gelir
    /// (`settings::lesson_hour_bounds`); "Gün Başlangıç Saati" ayarı kalktı.
    #[tokio::test]
    async fn board_hours_default_to_one_through_ten_when_max_daily_lessons_is_unset() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.day_start_hour, 1);
        assert_eq!(board.day_end_hour, 10);
    }

    #[tokio::test]
    async fn board_hours_follow_the_max_daily_lessons_setting() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        settings::set(&pool, "max_daily_lessons", "6").await.unwrap();

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.day_start_hour, 1);
        assert_eq!(board.day_end_hour, 7);
    }

    #[test]
    fn slot_keys_round_trip() {
        assert_eq!(parse_slot_key("3-10"), Some(Slot::new(3, 10)));
        assert_eq!(parse_slot_key("1-8"), Some(Slot::new(1, 8)));
    }

    /// Bozuk anahtar panik yerine None vermeli.
    #[test]
    fn malformed_slot_keys_are_ignored() {
        assert_eq!(parse_slot_key("bozuk"), None);
        assert_eq!(parse_slot_key("a-b"), None);
        assert_eq!(parse_slot_key(""), None);
    }


    /// Atama tahtasındaki havuz sayacı da TAM havuzdur: şeflik saatleri
    /// (alan şefi 10) Σ(saat × grup)'a eklenir.
    #[tokio::test]
    async fn assignment_board_pool_includes_chief_hours() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let row = TermBranchHoursInput {
            grade: "12/C".into(),
            branch: "Dal".into(),
            weekly_hours: 24,
            group_count: 2,
            is_group_manual: true,
        };
        teaching_load::replace_for_term(&pool, TERM, &[row]).await.unwrap();
        seed_teacher(&pool, "Alan", ChiefType::Department).await;

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.pool_hours, 24 * 2 + 10);
    }

    /// Kilit test (spec R5c, "üç yer"in biri): öğretmen kapasitesi de
    /// projeksiyondan okunur. `change_chief_type` yalnız kapıdan yazar; eski
    /// `teachers.chief_type` sütunu değişmese bile tahtadaki kapasite
    /// yeni şefliğe göre değişmeli.
    #[tokio::test]
    async fn teacher_capacity_on_the_board_follows_the_projection_not_the_legacy_column() {
        use crate::db::teaching_load_test_support::change_chief_type_in_planning;

        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;

        let before = load_board(&AppState { pool: pool.clone() }).await.unwrap();
        // Varsayılan ayar (migration 0001): "other" + büyükşehir => tavan 20.
        // Şefsizken bütçe (24) tavanı aştığı için kapasite tavanda KLİPLENİR: 20.
        assert_eq!(before.teachers[0].capacity, 20);

        // Dönem başından hemen sonraki bir tarih: gerçek "bugün"den önce
        // kalır, bu yüzden `current_as_of` (gerçek bugünü kullanır) her
        // koşulda bu değişikliği görür.
        change_chief_type_in_planning(&pool, teacher_id, ChiefType::Department, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).await;

        let after = load_board(&AppState { pool: pool.clone() }).await.unwrap();
        // Bölüm şefi 10 saat düşürür: bütçe 24-10=14, artık tavanın (20)
        // ALTINDA kaldığı için kapasite tam 14'e düşer (klipleme kalkar).
        assert_eq!(after.teachers[0].capacity, 14, "bölüm şefi 10 saat düşürmeli");

        let legacy: String = sqlx::query_scalar("SELECT chief_type FROM teachers WHERE id = ?1")
            .bind(teacher_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(legacy, "none", "eski sütun donuk kalmalı; okuma ona bakmıyor");
    }

    // --- Yazımlar tarihçeden geçer (brief: pano ↔ tarihçe kopukluğu) ---

    async fn a_company(pool: &SqlitePool, name: &str) -> i64 {
        companies::create(
            pool,
            &crate::domain::models::NewCompany {
                name: name.into(),
                contact_first_name: String::new(),
                contact_last_name: String::new(),
                phone: String::new(),
                email: String::new(),
                address_text: "Test adres".into(),
                latitude: None,
                longitude: None,
                one_way_distance_km: Some(5.0),
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    fn assignment_input(teacher_id: i64, company_id: i64, day: i64, hour: i64) -> NewAssignment {
        NewAssignment { teacher_id, company_id, visit_day: day, visit_hour: hour, is_forced: false, force_reason: None }
    }

    /// Planlama evresindeki (dönem başlamadan önceki) sabit bir "bugün":
    /// `_for_term` yardımcıları `today`yi parametre alır (bkz. üretim
    /// kodundaki yorum), bu yüzden testler gerçek takvim gününe bağlı KALMAZ.
    fn planning_today() -> NaiveDate {
        ymd(2026, 8, 15)
    }

    /// `assign_company` açık `coordination_periods` satırını yazar, donmuş
    /// `assignments`e dokunmaz; tarihçede bir olay oluşur.
    #[tokio::test]
    async fn assign_company_writes_the_open_projection_row_not_the_frozen_legacy_table() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool: pool.clone() };
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        let company_id = a_company(&pool, "İşletme A").await;

        assign_company_for_term(&state, assignment_input(teacher_id, company_id, 3, 4), None, None, planning_today())
            .await
            .unwrap();

        let saved = assignments::get_for_company(&pool, company_id, TERM).await.unwrap().unwrap();
        assert_eq!((saved.visit_day, saved.visit_hour), (3, 4));
        let legacy_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assignments").fetch_one(&pool).await.unwrap();
        assert_eq!(legacy_rows, 0, "donmuş eski tabloya hiç yazılmamalı");
        let events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE stream = 'coordination' AND subject_id = ?1")
                .bind(company_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(events > 0, "tarihçede bir olay oluşmalı");
    }

    /// Aynı işletme başka bir öğretmene yeniden atanınca YERİNDE taşınır
    /// (eski `assign`in "reassigning moves it" davranışı korunur).
    #[tokio::test]
    async fn reassigning_a_company_moves_it() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool: pool.clone() };
        let first = seed_teacher(&pool, "Bir", ChiefType::None).await;
        let second = seed_teacher(&pool, "Iki", ChiefType::None).await;
        let company_id = a_company(&pool, "İşletme A").await;

        assign_company_for_term(&state, assignment_input(first, company_id, 1, 1), None, None, planning_today()).await.unwrap();
        assign_company_for_term(&state, assignment_input(second, company_id, 5, 7), None, None, planning_today()).await.unwrap();

        let rows = assignments::list(&pool, TERM).await.unwrap();
        assert_eq!(rows.len(), 1, "ikinci kayıt açılmamalı, yerinde taşınmalı");
        assert_eq!(rows[0].teacher_id, second);
        assert_eq!(rows[0].visit_day, 5);
    }

    /// Zorlama gerekçesi boşsa reddedilir (eski `db/assignments.rs::validate`in
    /// korunması gereken kontrolü — brief karşılaştırma tablosu).
    #[tokio::test]
    async fn assign_company_rejects_forcing_without_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool: pool.clone() };
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        let company_id = a_company(&pool, "İşletme A").await;
        let mut forced = assignment_input(teacher_id, company_id, 1, 1);
        forced.is_forced = true;

        let err = assign_company_for_term(&state, forced, None, None, planning_today()).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    /// Dönem başladıysa `effectiveDate` olmadan atama `Validation` ile
    /// reddedilir; tarihle birlikte kabul edilir.
    #[tokio::test]
    async fn assign_company_requires_an_effective_date_once_the_term_has_started() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool: pool.clone() };
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        let company_id = a_company(&pool, "İşletme A").await;
        let running_today = ymd(2026, 11, 10);

        let err = assign_company_for_term(&state, assignment_input(teacher_id, company_id, 1, 1), None, None, running_today)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));

        assign_company_for_term(
            &state,
            assignment_input(teacher_id, company_id, 1, 1),
            Some("2026-11-03".to_string()),
            None,
            running_today,
        )
        .await
        .unwrap();
        assert!(assignments::get_for_company(&pool, company_id, TERM).await.unwrap().is_some());
    }

    /// `unassign_company`, eski `assignments::unassign` gibi atama YOKSA
    /// sessizce geçer (idempotent) — `EndCoordination`in `FactNotTrueAtDate`
    /// reddine takılmaz (doğrulama kaybı yok).
    #[tokio::test]
    async fn unassign_company_is_a_no_op_without_an_existing_assignment() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool };
        let company_id = a_company(&state.pool, "İşletme A").await;

        unassign_company_for_term(&state, company_id, None, None, planning_today()).await.unwrap();
    }

    /// Var olan bir atama `unassign_company` ile açık projeksiyon satırını kapatır.
    #[tokio::test]
    async fn unassign_company_ends_an_existing_open_assignment() {
        // Atama dönem başında (planlamada) yapılır; bitiş dönem başladıktan
        // SONRAKİ bir tarihte istenir — aynı gün "başlat ve aynı anda bitir"
        // `end_coordination`in "önceki durum" denetimine (spec §5.1,
        // `state_before`) anlamsızca çarpardı; gerçek kullanım da böyledir.
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool: pool.clone() };
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        let company_id = a_company(&pool, "İşletme A").await;
        let running_today = ymd(2026, 11, 10);
        assign_company_for_term(&state, assignment_input(teacher_id, company_id, 1, 1), None, None, planning_today()).await.unwrap();

        unassign_company_for_term(&state, company_id, Some("2026-11-05".to_string()), None, running_today).await.unwrap();

        assert!(assignments::get_for_company(&pool, company_id, TERM).await.unwrap().is_none());
    }

    /// `clear_assignments` tasarım gereği yalnız planlama evresinde çalışır;
    /// dönem başladıysa `ClearCoordination`in `PlanningOnly` reddi geçerlidir
    /// (brief: "dönem başladıysa servis ne diyorsa o" — eski davranıştan
    /// BİLİNÇLİ bir kısıtlama, bkz. üretim kodundaki yorum).
    #[tokio::test]
    async fn clear_assignments_is_rejected_once_the_term_has_started() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool };
        let running_today = ymd(2026, 11, 10);

        let err = clear_assignments_for_term(&state, None, None, running_today).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    /// Planlama evresinde ise (dönem başlamadan önce) `clear_assignments`
    /// tüm açık atamaları kapatır — eski `clear_term`in davranışı.
    #[tokio::test]
    async fn clear_assignments_clears_open_assignments_while_planning() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        let state = AppState { pool: pool.clone() };
        let teacher_id = seed_teacher(&pool, "Ada", ChiefType::None).await;
        let company_id = a_company(&pool, "İşletme A").await;
        assign_company_for_term(&state, assignment_input(teacher_id, company_id, 1, 1), None, None, planning_today()).await.unwrap();

        clear_assignments_for_term(&state, None, None, planning_today()).await.unwrap();

        assert!(assignments::list(&pool, TERM).await.unwrap().is_empty());
    }
