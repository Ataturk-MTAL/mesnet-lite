//! Dönem içi değişiklik tarihçesinin saf çekirdeği (Faz A, spec §3).
//!
//! Bu modül I/O içermez: olay türleri, akış başına `apply` ve katlama
//! burada yaşar. Veritabanı erişimi `db::` katmanındadır (R3); karar verme
//! (`decide`) ve etki özeti R2'de bu modüle eklenir.

pub mod apply;
pub mod events;
pub mod rejection;
pub mod timeline;
