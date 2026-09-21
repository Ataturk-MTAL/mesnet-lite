//! İşletme Belirleme Komisyon Tutanağı'nın PDF çıktısı.
//!
//! Düzen `templates/commission_minutes.typ` şablonundadır; veri Excel çıktısıyla
//! aynı `MinutesData`'dır ve `sys.inputs.data` üzerinden JSON olarak aktarılır.
//! Motor ve derleme yardımcısı (`render_pdf`) diğer raporlarla ortaktır.

use crate::error::AppResult;
use crate::services::commission_minutes::{build_minutes_data, MinutesData};
use crate::services::pdf_report::{render_pdf, FONT_BOLD, FONT_REGULAR};
use sqlx::SqlitePool;
use std::sync::OnceLock;
use typst_as_lib::{TypstEngine, TypstTemplateMainFile};

const MINUTES_TEMPLATE: &str = include_str!("../../templates/commission_minutes.typ");

fn minutes_engine() -> &'static TypstEngine<TypstTemplateMainFile> {
    static ENGINE: OnceLock<TypstEngine<TypstTemplateMainFile>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        TypstEngine::builder()
            .main_file(MINUTES_TEMPLATE)
            .fonts([FONT_REGULAR, FONT_BOLD])
            .build()
    })
}

/// Hazır bir `MinutesData`'yı PDF baytlarına çevirir (saf; veritabanı yok).
pub fn render_minutes_pdf(data: &MinutesData) -> AppResult<Vec<u8>> {
    render_pdf(minutes_engine(), data)
}

/// Dönemin komisyon tutanağını PDF olarak üretir.
pub async fn build_minutes_pdf(pool: &SqlitePool, term: &str) -> AppResult<Vec<u8>> {
    render_minutes_pdf(&build_minutes_data(pool, term).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::settings;
    use crate::services::commission_minutes::FIELD_NAME_KEY;
    use crate::services::commission_minutes_test_support::*;

    #[tokio::test]
    async fn empty_term_produces_a_valid_pdf() {
        let (_dir, pool) = test_pool().await;

        let pdf = build_minutes_pdf(&pool, TERM).await.unwrap();

        assert!(pdf.starts_with(b"%PDF"), "PDF imzasıyla başlamalı");
        assert!(
            pdf.len() > 2_000,
            "PDF beklenenden küçük: {} bayt",
            pdf.len()
        );
    }

    /// Tek satırlık ve çok satırlık (rowspan) gruplar, atanmamış, fahri ve
    /// uzaklıksız işletme birlikte derlenmeli; şablon hata verirse test düşer.
    #[tokio::test]
    async fn full_scenario_compiles_and_is_larger_than_the_empty_document() {
        let (_empty_dir, empty_pool) = test_pool().await;
        let empty = build_minutes_pdf(&empty_pool, TERM).await.unwrap();

        let (_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;
        settings::set(&pool, FIELD_NAME_KEY, "Elektrik-Elektronik Teknolojisi")
            .await
            .unwrap();
        let filled = build_minutes_pdf(&pool, TERM).await.unwrap();

        assert!(filled.starts_with(b"%PDF"));
        assert!(
            filled.len() > empty.len(),
            "dolu tutanak boş olandan büyük olmalı"
        );
    }

    /// Ç, Ğ, İ, Ş gibi büyük harfli Türkçe adlar (başlık zaten BÜYÜK HARF) PDF'e
    /// dönüşebilmeli. Glif kapsamını `pdf_report` testleri fontun cmap
    /// tablosundan doğrular; burada şablonun bu metinlerle derlendiği görülür.
    #[tokio::test]
    async fn turkish_capitals_in_every_field_compile() {
        let (_dir, pool) = test_pool().await;
        settings::set(
            &pool,
            "school_name",
            "Şükrü Saracoğlu Mesleki ve Teknik Anadolu Lisesi",
        )
        .await
        .unwrap();
        settings::set(&pool, "principal_name", "Çağıl İşıközü")
            .await
            .unwrap();
        let teacher = seed_teacher(&pool, "Işıl", "ÇAĞIL İŞIKÖZÜ").await;
        let company = seed_company(
            &pool,
            "ÇELİK ÖĞÜT SANAYİ A.Ş. — Gıda ve Şişeleme",
            Some(3.5),
        )
        .await;
        seed_student(&pool, Some(company), "Gökçe", "Ünlü").await;
        seed_hours(&pool, company, 6, false).await;
        seed_assignment(&pool, teacher, company, 3, 4).await;

        let pdf = build_minutes_pdf(&pool, TERM).await.unwrap();

        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 2_000);
    }

    /// Uzun tablo (birden çok sayfa, başlık yinelemesi) da derlenmeli.
    #[tokio::test]
    async fn a_multi_page_table_compiles() {
        let (_dir, pool) = test_pool().await;
        seed_long_table(&pool).await;

        let pdf = build_minutes_pdf(&pool, TERM).await.unwrap();

        assert!(pdf.starts_with(b"%PDF"));
    }

    #[tokio::test]
    async fn unreadable_term_is_reported_not_swallowed() {
        let (_dir, pool) = test_pool().await;

        assert!(build_minutes_pdf(&pool, "").await.is_err());
    }

    /// Brief'teki gerçek senaryo: 1 alan şefi + 11 alan öğretmeni imza
    /// şeridine sığmalı. Adlar iki sütuna bölünür (bkz. şablon); şablon hata
    /// verirse (taşma, bozuk grid) bu test düşer.
    #[tokio::test]
    async fn a_full_signature_roster_of_twelve_teachers_compiles() {
        use crate::domain::models::ChiefType;

        let (_dir, pool) = test_pool().await;
        seed_teacher_with_chief(&pool, "Ayşe", "Yılmaz", ChiefType::Department).await;
        for (first, last) in [
            ("Mehmet", "Öztürk"),
            ("Zeynep", "Çelik"),
            ("Ali", "Şahin"),
            ("Fatma", "Güneş"),
            ("Kemal", "İyi"),
            ("Elif", "Ünlü"),
            ("Burak", "Işık"),
            ("Ece", "Arı"),
            ("Deniz", "Doğan"),
        ] {
            seed_teacher_with_chief(&pool, first, last, ChiefType::WorkshopLab).await;
        }
        for (first, last) in [("Selin", "Ak"), ("Emre", "Bulut")] {
            seed_teacher_with_chief(&pool, first, last, ChiefType::None).await;
        }

        let pdf = build_minutes_pdf(&pool, TERM).await.unwrap();

        assert!(pdf.starts_with(b"%PDF"));
    }

    /// Elle inceleme yardımcısı: tohumlu senaryodan iki örnek dosya yazar.
    /// Hedef klasör `COMMISSION_MINUTES_SAMPLE_DIR` ortam değişkeninden gelir;
    /// normal `cargo test` çalışmasında atlanır.
    ///
    /// `COMMISSION_MINUTES_SAMPLE_DIR=/yol cargo test --lib write_sample_files -- --ignored`
    #[tokio::test]
    #[ignore = "elle inceleme için örnek dosya yazar"]
    async fn write_sample_files() {
        use crate::domain::models::ChiefType;

        let dir = std::env::var("COMMISSION_MINUTES_SAMPLE_DIR")
            .expect("COMMISSION_MINUTES_SAMPLE_DIR ortam değişkeni ayarlanmalı");
        let (_db_dir, pool) = test_pool().await;
        seed_full_scenario(&pool).await;
        settings::set(&pool, FIELD_NAME_KEY, "Elektrik-Elektronik Teknolojisi")
            .await
            .unwrap();
        settings::set(&pool, "principal_name", "Ömer Yiğit")
            .await
            .unwrap();

        // İmza şeridi örneği: 1 alan şefi + 9 atölye/laboratuvar şefi + 2 sade
        // öğretmen (brief'teki gerçek senaryo). `seed_full_scenario`'nun
        // atadığı koordinatörlerden (Ayşe Yılmaz, Mehmet Öztürk) farklı adlar
        // seçildi ki elle incelemede aynı isim iki farklı rolde görünmesin.
        seed_teacher_with_chief(&pool, "Nur", "Aydın", ChiefType::Department).await;
        for (first, last) in [
            ("Cem", "Bozkurt"),
            ("Derya", "Çınar"),
            ("Emre", "Doğan"),
            ("Gül", "Erdem"),
            ("Halil", "Fındık"),
            ("İpek", "Güler"),
            ("Kaan", "Hoşgör"),
            ("Leyla", "Işık"),
            ("Onur", "Şen"),
        ] {
            seed_teacher_with_chief(&pool, first, last, ChiefType::WorkshopLab).await;
        }
        for (first, last) in [("Pınar", "Ünal"), ("Rıza", "Vural")] {
            seed_teacher_with_chief(&pool, first, last, ChiefType::None).await;
        }

        let pdf = build_minutes_pdf(&pool, TERM).await.unwrap();
        let xlsx = crate::services::commission_minutes_xlsx::build_minutes_xlsx(&pool, TERM)
            .await
            .unwrap();

        std::fs::write(format!("{dir}/tutanak-ornek.pdf"), pdf).unwrap();
        std::fs::write(format!("{dir}/tutanak-ornek.xlsx"), xlsx).unwrap();
    }
}
