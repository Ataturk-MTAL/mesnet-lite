    use super::*;
    use crate::db::init_pool;
    use crate::db::teaching_load_test_support::{seed_teacher, ymd, TERM};
    use crate::domain::models::ChiefType;
    use crate::error::AppError;
    use sqlx::SqlitePool;
    use teaching_load::TermBranchHours;

    fn saved(id: i64, grade: &str, branch: &str, weekly: i64, groups: i64) -> TermBranchHours {
        TermBranchHours {
            id,
            term: "2026-2027/1".into(),
            grade: grade.into(),
            branch: branch.into(),
            weekly_hours: weekly,
            group_count: groups,
            is_group_manual: true,
            created_at: "2026-09-01T00:00:00Z".into(),
            updated_at: "2026-09-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn merge_marks_saved_rows_as_not_suggested() {
        let existing = vec![saved(1, "12/C", "Elektronik Haberleşme", 24, 2)];
        let rows = merge_with_suggestions(existing, vec![], &BranchStudentCounts::new());

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, Some(1));
        assert!(!rows[0].is_suggested);
    }

    /// Öğrencisi olup henüz satırı olmayan çift öneri olarak eklenir.
    #[test]
    fn merge_adds_suggestion_for_branch_without_a_saved_row() {
        let rows = merge_with_suggestions(
            vec![],
            vec![("12/D".to_string(), "Endüstriyel Bakım Onarım".to_string())],
            &BranchStudentCounts::new(),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, None);
        assert!(rows[0].is_suggested);
        assert_eq!(rows[0].weekly_hours, 0);
        assert_eq!(rows[0].group_count, 0);
    }

    /// Zaten kayıtlı bir (sınıf, dal) için ikinci bir öneri satırı EKLENMEZ.
    #[test]
    fn merge_does_not_duplicate_a_branch_that_already_has_a_saved_row() {
        let existing = vec![saved(1, "12/C", "Elektronik Haberleşme", 24, 2)];
        let rows = merge_with_suggestions(
            existing,
            vec![("12/C".to_string(), "Elektronik Haberleşme".to_string())],
            &BranchStudentCounts::new(),
        );

        assert_eq!(rows.len(), 1, "aynı çift için ikinci satır eklenmemeli");
        assert!(!rows[0].is_suggested);
    }

    #[test]
    fn merge_sorts_rows_by_grade_then_branch() {
        let rows = merge_with_suggestions(
            vec![],
            vec![
                ("12/D".to_string(), "Dal B".to_string()),
                ("12/C".to_string(), "Dal A".to_string()),
            ],
            &BranchStudentCounts::new(),
        );

        assert_eq!(rows[0].grade, "12/C");
        assert_eq!(rows[1].grade, "12/D");
    }

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("test.db")).await.unwrap();
        (dir, pool)
    }

    /// 24×2 + 24×1 = 72 saatlik Σ ve alan şefi (10) + atölye şefi (6) = 16.
    async fn pool_with_chiefs() -> (tempfile::TempDir, SqlitePool) {
        let (dir, pool) = test_pool().await;
        let input = |grade: &str, weekly, groups| TermBranchHoursInput {
            grade: grade.into(),
            branch: "Dal".into(),
            weekly_hours: weekly,
            group_count: groups,
            is_group_manual: true,
        };
        teaching_load::replace_for_term(&pool, TERM, &[input("12/C", 24, 2), input("12/D", 24, 1)])
            .await
            .unwrap();
        seed_teacher(&pool, "Alan", ChiefType::Department).await;
        seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;
        (dir, pool)
    }

    /// Ders yükü tahtası: `poolHours` toplamdır, iki kalemi ayrı da verir.
    #[tokio::test]
    async fn teaching_load_board_pool_is_branch_hours_plus_chief_hours() {
        let (_dir, pool) = pool_with_chiefs().await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!(board.branch_hours, 72);
        assert_eq!(board.chief_planning_hours, 16);
        assert_eq!(board.pool_hours, 88);
        assert_eq!(board.branch_hours + board.chief_planning_hours, board.pool_hours);
    }

    /// İşletme takdir tahtası ve aşım uyarısı TAM havuza (şeflik dahil) bakar.
    #[tokio::test]
    async fn hours_board_pool_includes_chief_hours() {
        let (_dir, pool) = pool_with_chiefs().await;

        let board = load_board(&AppState { pool }).await.unwrap();

        assert_eq!(board.pool_hours, 72 + 16);
    }

    /// Otomatik dağıtım tam havuzu paylaştırır: tek işletmenin tavanı havuzdan
    /// büyükse dağıtılan saat şeflik dahil toplam havuza eşit olmalı.
    #[tokio::test]
    async fn auto_distribute_shares_the_full_pool_including_chief_hours() {
        let (_dir, pool) = pool_with_chiefs().await;
        let rows = vec![AutoDistributeRow {
            company_id: 1,
            max_hours: 1000,
            student_count: 5,
            is_locked: false,
            current_awarded: 0,
            is_honorary: false,
        }];

        let outcome = distribute_for_term(&pool, TERM, rows).await.unwrap();

        assert_eq!(outcome.distributed_hours, 72 + 16);
    }

    /// Şeflik yokken havuz yalnız Σ(saat × grup); kayıtlı satırlar bozulmaz.
    #[tokio::test]
    async fn teaching_load_board_pool_is_only_the_branch_sum_without_chiefs() {
        let (_dir, pool) = test_pool().await;
        teaching_load::replace_for_term(
            &pool,
            TERM,
            &[TermBranchHoursInput { grade: "12/C".into(), branch: "Dal".into(), weekly_hours: 24, group_count: 2, is_group_manual: true }],
        )
        .await
        .unwrap();

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!((board.branch_hours, board.chief_planning_hours, board.pool_hours), (48, 0, 48));
    }

    // --- Otomatik grup sayısı (Norm Kadro Yön. MADDE 22/1-ç, migration 0007) ---

    fn counts(entries: &[(&str, &str, i64)]) -> BranchStudentCounts {
        entries
            .iter()
            .map(|(grade, branch, n)| ((grade.to_string(), branch.to_string()), *n))
            .collect()
    }

    fn auto_input(grade: &str, branch: &str, weekly: i64) -> TermBranchHoursInput {
        TermBranchHoursInput {
            grade: grade.into(),
            branch: branch.into(),
            weekly_hours: weekly,
            group_count: 99, // otomatik satırda yok sayılmalı
            is_group_manual: false,
        }
    }

    async fn n_students(pool: &SqlitePool, grade: &str, branch: &str, count: usize) {
        for _ in 0..count {
            crate::db::students::create(
                pool,
                &crate::domain::models::NewStudent {
                    first_name: "Test".into(),
                    last_name: "Ogrenci".into(),
                    student_no: None,
                    grade: grade.into(),
                    branch: branch.into(),
                    submitted_at: None,
                    term: TERM.into(),
                },
            )
            .await
            .unwrap();
        }
    }

    /// Öneri satırı (kayıtsız): grup sayısı tablodan gelir, saat 0 kalır.
    #[test]
    fn merge_suggestion_carries_the_automatic_group_count_and_zero_hours() {
        let rows = merge_with_suggestions(
            vec![],
            vec![("12/D".to_string(), "Dal B".to_string())],
            &counts(&[("12/D", "Dal B", 17)]),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].group_count, 2);
        assert_eq!(rows[0].auto_group_count, 2);
        assert!(!rows[0].is_group_manual);
        assert_eq!(rows[0].weekly_hours, 0);
    }

    /// Otomatik kayıtlı satır: `groupCount` etkin (hesaplanan) değerdir,
    /// saklanan önbellek değil.
    #[test]
    fn merge_automatic_saved_row_shows_the_computed_group_count_not_the_cache() {
        let mut stored = saved(1, "12/C", "Dal A", 24, 1);
        stored.is_group_manual = false;

        let rows = merge_with_suggestions(vec![stored], vec![], &counts(&[("12/C", "Dal A", 20)]));

        assert_eq!(rows[0].group_count, 2, "önbellek 1 ama 20 öğrenci ⇒ 2 grup");
        assert_eq!(rows[0].auto_group_count, 2);
        assert!(!rows[0].is_group_manual);
    }

    /// Elle kayıtlı satır: `groupCount` saklanan değerdir; `autoGroupCount`
    /// yine tablodan hesaplanıp gösterilir ("otomatiğe dön" önizlemesi).
    #[test]
    fn merge_manual_saved_row_keeps_the_stored_count_and_still_reports_the_auto_one() {
        let rows = merge_with_suggestions(
            vec![saved(1, "12/C", "Dal A", 24, 3)],
            vec![],
            &counts(&[("12/C", "Dal A", 16)]),
        );

        assert_eq!(rows[0].group_count, 3);
        assert_eq!(rows[0].auto_group_count, 1);
        assert!(rows[0].is_group_manual);
    }

    /// Gerçek senaryo tahtada: 12/C 16 öğrenci, 12/D'de iki dal 7 ve 9 öğrenci,
    /// hepsi otomatik, ders saati 24 ⇒ Σ 72; atölye şefi 6 ile toplam 78.
    #[tokio::test]
    async fn teaching_load_board_real_scenario_is_72_branch_hours_and_78_in_total() {
        let (_dir, pool) = test_pool().await;
        n_students(&pool, "12/C", "Elektronik Haberleşme", 16).await;
        n_students(&pool, "12/D", "Endüstriyel Bakım Onarım", 7).await;
        n_students(&pool, "12/D", "Bilişim Teknolojileri", 9).await;
        teaching_load::replace_for_term(
            &pool,
            TERM,
            &[
                auto_input("12/C", "Elektronik Haberleşme", 24),
                auto_input("12/D", "Endüstriyel Bakım Onarım", 24),
                auto_input("12/D", "Bilişim Teknolojileri", 24),
            ],
        )
        .await
        .unwrap();
        seed_teacher(&pool, "Atolye", ChiefType::WorkshopLab).await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!((board.branch_hours, board.chief_planning_hours, board.pool_hours), (72, 6, 78));
        assert!(board.rows.iter().all(|r| r.group_count == 1 && r.auto_group_count == 1));
        assert!(board.rows.iter().all(|r| !r.is_group_manual && !r.is_suggested));
    }

    /// Kaydedilmemiş öneri satırı `autoGroupCount` ile dolar ama saati 0
    /// olduğu için havuza katkı vermez.
    #[tokio::test]
    async fn suggested_rows_are_filled_with_the_auto_count_and_add_nothing_to_the_pool() {
        let (_dir, pool) = test_pool().await;
        n_students(&pool, "12/C", "Dal A", 20).await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!(board.rows.len(), 1);
        let suggested = &board.rows[0];
        assert!(suggested.is_suggested && !suggested.is_group_manual);
        assert_eq!((suggested.group_count, suggested.auto_group_count), (2, 2));
        assert_eq!(suggested.weekly_hours, 0);
        assert_eq!((board.branch_hours, board.pool_hours), (0, 0));
    }

    /// Otomatik kayıtlı satır tahtada öğrenci sayısını izler; elle satır izlemez.
    #[tokio::test]
    async fn board_shows_automatic_rows_following_students_and_manual_rows_fixed() {
        let (_dir, pool) = test_pool().await;
        n_students(&pool, "12/C", "Dal A", 16).await;
        n_students(&pool, "12/D", "Dal B", 16).await;
        let mut manual = auto_input("12/D", "Dal B", 24);
        manual.is_group_manual = true;
        manual.group_count = 3;
        teaching_load::replace_for_term(&pool, TERM, &[auto_input("12/C", "Dal A", 24), manual])
            .await
            .unwrap();
        n_students(&pool, "12/C", "Dal A", 1).await;
        n_students(&pool, "12/D", "Dal B", 1).await;

        let board = build_teaching_load_board(&pool, TERM, ymd(2026, 10, 1)).await.unwrap();

        assert_eq!(board.rows[0].group_count, 2, "12/C otomatik: 17 öğrenci ⇒ 2");
        assert_eq!(board.rows[1].group_count, 3, "12/D elle: 3 korunur");
        assert_eq!(board.rows[1].auto_group_count, 2);
        assert_eq!(board.branch_hours, 24 * 2 + 24 * 3);
    }

    // --- Havuz aşımı: `save_company_hours` yolu (kullanıcı kuralı "havuz aşılamaz") ---

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
                // Mesafe BİLİNMİYOR: `cap_for` bu yüzden hiç tavan koymaz —
                // TEK ŞARTIYLA öğrenci sayısı > 0 (`cap_for`, 0 öğrencide
                // mesafeden BAĞIMSIZ `Some(0)` döner). Bu testler yalnız
                // HAVUZ aşımını sınar; tavan hiç devreye girmemeli.
                one_way_distance_km: None,
                district: String::new(),
                notes: String::new(),
            },
        )
        .await
        .unwrap()
        .id
    }

    /// `a_company` + tavanı devre dışı bırakmak için TEK bir öğrenci
    /// (yerleştirme `execute_change` ÜZERİNDEN, tek doğruluk kaynağı
    /// `student_placements` projeksiyonudur).
    async fn a_company_without_a_cap(pool: &SqlitePool, name: &str) -> i64 {
        let company_id = a_company(pool, name).await;
        let req = ChangeRequest {
            term: TERM.to_string(),
            effective_date: None,
            document_date: None,
            reason: "test".into(),
            command: ChangeCommand::CreateStudent {
                student: crate::domain::history::decide::NewStudentInput {
                    first_name: name.to_string(),
                    last_name: "Öğrenci".into(),
                    student_no: None,
                    grade: "12/C".into(),
                    branch: "Elektronik Haberleşme".into(),
                    submitted_at: None,
                },
                company_id: Some(company_id),
            },
        };
        let outcome = crate::services::change_service::execute_change(
            pool,
            req,
            crate::services::change_service::ChangeMode::Commit { expected_high_water: None },
            ymd(2026, 8, 15),
        )
        .await
        .unwrap();
        assert!(matches!(outcome, crate::services::change_service::ChangeOutcome::Committed { .. }));
        company_id
    }

    /// Tek dal satırıyla havuzu istenen değere sabitler (grup 1, saat = havuz).
    async fn set_pool_hours(pool: &SqlitePool, hours: i64) {
        teaching_load::replace_for_term(
            pool,
            TERM,
            &[TermBranchHoursInput { grade: "12/C".into(), branch: "Dal".into(), weekly_hours: hours, group_count: 1, is_group_manual: true }],
        )
        .await
        .unwrap();
    }

    fn hours_input(company_id: i64, awarded: i64) -> HoursInput {
        HoursInput { company_id, max_hours_snapshot: 1000, awarded_hours: awarded, is_honorary: false, is_locked: false, notes: String::new() }
    }

    /// `save_hours_for_term`i planlama evresinde (dönem başlamadan önceki
    /// sabit bir "bugün" ile), tarih vermeden çağırır — tarih zorunluluğuna
    /// (`EffectiveDateRequired`) takılmadan; gerçek takvim gününe bağlı KALINMAZ.
    async fn save(pool: &SqlitePool, rows: &[HoursInput]) -> AppResult<()> {
        save_hours_for_term(pool, TERM, rows, None, None, ymd(2026, 8, 15)).await
    }

    /// Havuz 100, İşletme A zaten 90 almış; B'ye +20 vermek toplamı 110'a
    /// çıkarır — reddedilir, HİÇBİR şey yazılmaz. Yol artık tarihçe
    /// kapısından geçiyor: `company_hour_periods` projeksiyonu denetlenir,
    /// donmuş `company_term_hours`e hiç dokunulmaz.
    #[tokio::test]
    async fn save_company_hours_rejects_when_pool_would_be_exceeded() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 100).await;
        let a = a_company_without_a_cap(&pool, "İşletme A").await;
        let b = a_company_without_a_cap(&pool, "İşletme B").await;
        save(&pool, &[hours_input(a, 90)]).await.unwrap();

        let err = save(&pool, &[hours_input(b, 20)]).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 90, "reddedilen istekte hiçbir şey yazılmamalı");
    }

    /// Tam havuza eşitlemek (90 + 10 = 100) SINIR DAHİL kabul edilir.
    #[tokio::test]
    async fn save_company_hours_accepts_reaching_the_pool_exactly() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 100).await;
        let a = a_company_without_a_cap(&pool, "İşletme A").await;
        let b = a_company_without_a_cap(&pool, "İşletme B").await;
        save(&pool, &[hours_input(a, 90)]).await.unwrap();

        save(&pool, &[hours_input(b, 10)]).await.unwrap();
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 100);
    }

    /// Havuz tanımlanmamışsa (`0`) aşım denetimi hiç yapılmaz.
    #[tokio::test]
    async fn save_company_hours_skips_the_pool_check_when_the_pool_is_undefined() {
        let (_dir, pool) = test_pool().await;
        let a = a_company_without_a_cap(&pool, "İşletme A").await;
        let large = HoursInput { max_hours_snapshot: 10_000, ..hours_input(a, 10_000) };

        save(&pool, &[large]).await.unwrap();
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 10_000);
    }

    /// Aşımı AZALTAN bir düzenleme (100 → 80, havuz 80) kabul edilir. İlk
    /// (aşkın) değer `decide()`i BİLEREK atlayan `legacy_seed_test_support`
    /// ile kurulur — göçten kalma veriyi taklit eder; `decide()`in KENDİSİ
    /// artık bu tür bir aşımı YARATMAYA hiç izin vermez (bu tam da denetimin
    /// konusu).
    #[tokio::test]
    async fn save_company_hours_accepts_an_edit_that_reduces_the_total() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 80).await;
        let a = a_company_without_a_cap(&pool, "İşletme A").await;
        crate::db::legacy_seed_test_support::seed_hours(&pool, TERM, a, 100, false).await;

        save(&pool, &[hours_input(a, 80)]).await.unwrap();
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 80);
    }

    /// Göçten kalma veri zaten havuzu aşmışsa (60 > 50), aşımı ARTIRMAYAN bir
    /// düzenleme kilitlenmez; aşımı BÜYÜTEN bir düzenleme yine reddedilir.
    #[tokio::test]
    async fn save_company_hours_does_not_lock_a_pre_existing_overrun_but_still_rejects_a_further_increase() {
        let (_dir, pool) = test_pool().await;
        set_pool_hours(&pool, 50).await;
        let a = a_company_without_a_cap(&pool, "İşletme A").await;
        crate::db::legacy_seed_test_support::seed_hours(&pool, TERM, a, 60, false).await;

        save(&pool, &[hours_input(a, 60)]).await.expect("mevcut aşımı korumak kilitlenmemeli");

        let err = save(&pool, &[hours_input(a, 65)]).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "aşımı büyüten değişiklik yine reddedilmeli");
    }

    /// Brief testi: `save_company_hours` açık projeksiyon satırını yazar,
    /// donmuş `company_term_hours`e HİÇ dokunmaz, ve tarihçede bir olay oluşur.
    #[tokio::test]
    async fn save_company_hours_writes_the_open_projection_row_not_the_frozen_legacy_table() {
        let (_dir, pool) = test_pool().await;
        let a = a_company_without_a_cap(&pool, "İşletme A").await;

        save(&pool, &[hours_input(a, 5)]).await.unwrap();

        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 5, "pano projeksiyondan okumalı");
        let legacy_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM company_term_hours")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(legacy_rows, 0, "donmuş eski tabloya hiç yazılmamalı");
        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE stream = 'company_hours' AND subject_id = ?1")
            .bind(a)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(events > 0, "tarihçede bir olay oluşmalı");
    }

    /// Dönem başladıysa (planlama evresi bittiyse) `effectiveDate` olmadan
    /// kayıt `Validation` ile reddedilir; tarihle birlikte kabul edilir.
    #[tokio::test]
    async fn save_company_hours_requires_an_effective_date_once_the_term_has_started() {
        let (_dir, pool) = test_pool().await;
        let a = a_company_without_a_cap(&pool, "İşletme A").await;
        let running_today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();

        let err = save_hours_for_term(&pool, TERM, &[hours_input(a, 5)], None, None, running_today).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));

        save_hours_for_term(&pool, TERM, &[hours_input(a, 5)], Some("2026-11-03".to_string()), None, running_today)
            .await
            .unwrap();
        assert_eq!(company_hours::total_awarded(&pool, TERM).await.unwrap(), 5);
    }
