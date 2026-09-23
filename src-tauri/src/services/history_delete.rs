//! Tarihçeden ETKİSİZ kayıt silme (kullanıcı kararı 2026-09-23) — bugünkü
//! durumu (ve öğretmen ek ders SAATİNİ) DEĞİŞTİRMEYEN, yani ZATEN tamamen
//! geri alınmış bir kümeyi günlükten GERÇEKTEN kaldırır.
//!
//! `change_sets`/`change_events` normalde değişmezdir (migration 0006); bu,
//! migration 0012'nin `history_purge_guard` kapısıyla açılan TEK istisnadır.
//! Kapı yalnız BU fonksiyonun kendi `BEGIN IMMEDIATE` transaction'ı içinde,
//! silme adımları sürerken 1'dir; adımlar bitince 0'a döner. Herhangi bir
//! adım hata verirse `?` transaction'ı düşürür, sqlx otomatik ROLLBACK
//! yapar (bkz. `services/company_merge.rs`in AYNI deseni) — guard'ın
//! DB'de kalıcı olarak 1 kalması imkansızdır.
//!
//! Beş tarih aralıklı projeksiyon tablosu (`student_placements` vb.) her
//! satırda `source_event_id INTEGER NOT NULL REFERENCES change_events(id)`
//! taşır (migration 0006): silinecek bir olayı hâlâ gösteren bir projeksiyon
//! satırı varken o olayı silmek FK ihlaline yol açar. Bu yüzden dönemin
//! projeksiyonu SİLMEDEN ÖNCE tamamen boşaltılır (bu tablolara DELETE
//! yasak DEĞİLDİR — yalnız `BEFORE INSERT` çakışma denetimi vardır, bkz.
//! migration 0006) ve silme bittikten SONRA `projection::rebuild_term` ile
//! günlükten yeniden kurulur; sonuç silmeden önceki durumla BİREBİR aynı
//! olur (rebuild deterministiktir — bkz. `db/projection.rs`).

use sqlx::{SqliteConnection, SqlitePool};

use crate::db::{change_log, projection};
use crate::domain::history::deletion::is_deletable;
use crate::domain::history::events::StoredEvent;
use crate::error::{AppError, AppResult};

/// Beş tarih aralıklı projeksiyon tablosunun ADLARI — TEK burada listelenir
/// (bkz. `db/projection.rs`in dosya başı yorumu, aynı beşli).
const PROJECTION_TABLES: [&str; 5] =
    ["student_placements", "company_hour_periods", "coordination_periods", "teacher_load_periods", "teacher_schedule_periods"];

/// `delete_change_set({ changeSetId })` — brief "Silme işlemi".
pub async fn delete_change_set(pool: &SqlitePool, change_set_id: i64) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    delete_in(&mut tx, change_set_id).await?;
    tx.commit().await?;
    Ok(())
}

async fn delete_in(conn: &mut SqliteConnection, change_set_id: i64) -> AppResult<()> {
    let (term, kind) = load_change_set_summary(conn, change_set_id).await?;
    let events = change_log::load_term_events(conn, &term).await?;

    // Adım 1: kuralı YENİDEN doğrula (arayüzün gördüğü `isDeletable` ile
    // silme anı arasında başka bir yazıcı araya girmiş olabilir).
    if !is_deletable(&events, change_set_id, &kind) {
        return Err(AppError::Validation("Bu kayıt bugünkü durumu etkiliyor; silinemez. Önce geri alın.".to_string()));
    }

    // Silinecek olayları hâlâ gösteren projeksiyon satırı kalmasın (FK) —
    // bkz. dosya başı yorumu. `rebuild_term` zaten TÜMÜNÜ silip yeniden
    // yazıyor; burada yalnız o silmeyi olay silmelerinden ÖNCEYE alıyoruz.
    clear_term_projection(conn, &term).await?;

    set_guard(conn, true).await?;
    run_deletion(conn, change_set_id, &events).await?;
    set_guard(conn, false).await?;

    projection::rebuild_term(conn, &term).await
}

async fn load_change_set_summary(conn: &mut SqliteConnection, change_set_id: i64) -> AppResult<(String, String)> {
    let row: Option<(String, String)> = sqlx::query_as("SELECT term, kind FROM change_sets WHERE id = ?1")
        .bind(change_set_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or_else(|| AppError::Validation(format!("#{change_set_id} numaralı kayıt bulunamadı.")))
}

async fn clear_term_projection(conn: &mut SqliteConnection, term: &str) -> AppResult<()> {
    for table in PROJECTION_TABLES {
        sqlx::query(&format!("DELETE FROM {table} WHERE term = ?1")).bind(term).execute(&mut *conn).await?;
    }
    Ok(())
}

async fn set_guard(conn: &mut SqliteConnection, active: bool) -> AppResult<()> {
    sqlx::query("UPDATE history_purge_guard SET active = ?1 WHERE id = 1")
        .bind(active as i64)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Brief adım 2-4: markörleri, S'nin olaylarını ve S'yi sil; S'ye işaret
/// eden `revokes_change_set_id`leri NULL yap; boş kalan kümeleri de sil.
async fn run_deletion(conn: &mut SqliteConnection, change_set_id: i64, events: &[StoredEvent]) -> AppResult<()> {
    let own_event_ids: Vec<i64> = events.iter().filter(|e| e.change_set_id == change_set_id).map(|e| e.id).collect();

    // Adım 2: S'nin olaylarını hedefleyen işaret olaylarını (başka
    // kümelerde) sil; her markörün AİT OLDUĞU küme adım 4'ün adayıdır.
    let mut candidate_owners: Vec<i64> = Vec::new();
    for &target_id in &own_event_ids {
        if let Some(owner) = delete_marker_targeting(conn, target_id).await? {
            candidate_owners.push(owner);
        }
    }

    // Adım 3: S'nin olaylarını ve S'yi sil; S'ye işaret eden
    // `revokes_change_set_id`leri NULL yap.
    for &event_id in &own_event_ids {
        delete_event(conn, event_id).await?;
    }
    clear_revokes_pointing_at(conn, change_set_id).await?;
    delete_change_set_row(conn, change_set_id).await?;

    // Adım 4: 2. adımda markörü silinen kümelerden, artık HİÇ olayı
    // kalmayanları (salt geri-alma kümeleri) da sil. Opening bunlardan asla
    // OLAMAZ: bir açılış hiçbir zaman `revoked` işareti taşımaz.
    delete_now_empty_change_sets(conn, &candidate_owners).await
}

/// `target_event_id`i geri alan işaret olayını — HANGİ kümede olursa olsun —
/// bulur, onu `caused_by` ile referans alan kalan olayları önce NULL'a
/// çeker (dosya başı yorumu, `caused_by` FK'si), sonra markörü siler.
/// `revokes` UNIQUE olduğundan en fazla BİR markör olabilir.
async fn delete_marker_targeting(conn: &mut SqliteConnection, target_event_id: i64) -> AppResult<Option<i64>> {
    let marker: Option<(i64, i64)> = sqlx::query_as("SELECT id, change_set_id FROM change_events WHERE revokes = ?1")
        .bind(target_event_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some((marker_id, owner)) = marker else { return Ok(None) };
    delete_event(conn, marker_id).await?;
    Ok(Some(owner))
}

/// Bir olayı silmeden ÖNCE, onu `caused_by` ile referans alan HER kalan
/// satırı NULL'a çeker (FK) — `caused_by` yalnız görüntüleme amaçlıdır
/// (bkz. `domain/history/decide/revoke.rs`), bu yüzden NULL'a çekmek hiçbir
/// hesaplamayı bozmaz.
async fn delete_event(conn: &mut SqliteConnection, event_id: i64) -> AppResult<()> {
    sqlx::query("UPDATE change_events SET caused_by = NULL WHERE caused_by = ?1").bind(event_id).execute(&mut *conn).await?;
    sqlx::query("DELETE FROM change_events WHERE id = ?1").bind(event_id).execute(&mut *conn).await?;
    Ok(())
}

async fn clear_revokes_pointing_at(conn: &mut SqliteConnection, change_set_id: i64) -> AppResult<()> {
    sqlx::query("UPDATE change_sets SET revokes_change_set_id = NULL WHERE revokes_change_set_id = ?1")
        .bind(change_set_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn delete_change_set_row(conn: &mut SqliteConnection, change_set_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM change_sets WHERE id = ?1").bind(change_set_id).execute(&mut *conn).await?;
    Ok(())
}

async fn delete_now_empty_change_sets(conn: &mut SqliteConnection, candidate_ids: &[i64]) -> AppResult<()> {
    let mut unique: Vec<i64> = candidate_ids.to_vec();
    unique.sort_unstable();
    unique.dedup();

    for cs_id in unique {
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM change_events WHERE change_set_id = ?1")
            .bind(cs_id)
            .fetch_one(&mut *conn)
            .await?;
        // Bu kümeye işaret eden bir `revokes_change_set_id` KALAMAZ: yalnız
        // `revoke`/`correct` kümeleri bu alanı doldurur, ve `decide::revoke::
        // validate_revocable` zaten "bir geri alma kümesi tekrar geri
        // alınamaz" der — hiçbir küme bu kümeyi ASLA hedefleyemez, bu yüzden
        // burada ayrıca NULL'lamaya gerek yoktur.
        if remaining == 0 {
            delete_change_set_row(conn, cs_id).await?;
        }
    }
    Ok(())
}
