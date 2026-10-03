//! Dağıtım motoru: işletmeleri koordinatör öğretmenlere ve ders saati
//! hücrelerine yerleştirir, gerektiğinde işletme saatlerini tavanın altına
//! indirir. SAF alan kodu; veritabanı, duvar saati ve rastgelelik kullanmaz.
//!
//! Akış: `problem` (girdiyi normalleştir) → `construct` (açgözlü başlangıç) →
//! `improve` (yerel arama) → `output` (öneriyi ve gerekçeleri kur).

mod blocks;
mod construct;
mod explain;
mod improve;
mod model;
mod moves;
mod output;
mod problem;
mod state;

pub use model::*;

#[cfg(test)]
mod tests;

/// Süpürme sınırı: yakınsama genelde 3–5 süpürmede olur; sınır yalnız
/// sonsuz döngüye karşı güvencedir. Sayaç sınırı, duvar saati değil.
pub(crate) const MAX_SWEEPS: usize = 12;

/// Aday puanlaması ve blok taraması birimiyle üst sınır. 28 işletme × 14
/// öğretmen gerçek örneği bunun çok altında yakınsar; 300 × 100 gibi büyük
/// girdilerde arama bu sınırda keser ve uyarı döner.
pub(crate) const MAX_EVALUATIONS: u64 = 1_000_000;

/// Giriş noktası. Aynı girdi (sırası ne olursa olsun) aynı çıktıyı verir.
pub fn optimize(input: &EngineInput) -> AllocationProposal {
    let problem = problem::Problem::new(input);
    let mut state = state::State::new(&problem);
    construct::construct(&mut state);
    let report = improve::improve(&mut state);
    #[cfg(test)]
    state.assert_consistent("nihai");
    output::build(&problem, &mut state, &report)
}
