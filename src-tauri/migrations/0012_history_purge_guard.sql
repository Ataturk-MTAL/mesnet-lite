-- Tarihçeden ETKİSİZ kayıt silme (kullanıcı kararı 2026-09-23): bugünkü
-- durumu (ve öğretmen ek ders SAATİNİ) DEĞİŞTİRMEYEN — yani zaten TAMAMEN
-- geri alınmış — kayıtlar denetim listesinden GERÇEKTEN silinebilir.
--
-- Migration 0006'daki `change_sets`/`change_events` DEĞİŞMEZLİK
-- tetikleyicileri (`trg_*_no_delete`, `trg_*_no_update`) normalde HER
-- silme/güncellemeyi reddeder — bu doğru varsayılan DEĞİŞMİYOR. Tek satırlık
-- bu "kapı" tablosu, `services/history_delete.rs`in KENDİ `BEGIN IMMEDIATE`
-- transaction'ı İÇİNDE, yalnızca silme adımları sürerken `active`'i 1
-- yapmasını sağlar; adımlar bitince 0'a döner, hata olursa transaction'ın
-- ROLLBACK'i guard'ı da geri alır (SQLite'ta DDL/DML aynı transaction'a
-- dahildir). Guard 0 iken davranış AYNEN eskisi gibidir (bkz.
-- `db/change_log.rs::change_log_rejects_update_and_delete`, hâlâ yeşil).
--
-- `CHECK (id = 1)`: tablo TEK satırlıdır, ikinci bir "kapı" anlamsızdır.
PRAGMA foreign_keys = ON;

CREATE TABLE history_purge_guard (
    id     INTEGER PRIMARY KEY CHECK (id = 1),
    active INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1))
);

INSERT INTO history_purge_guard (id, active) VALUES (1, 0);

-- ============================================================
-- change_sets — DELETE guard'a bağlanır
-- ============================================================
DROP TRIGGER trg_change_sets_no_delete;
CREATE TRIGGER trg_change_sets_no_delete
BEFORE DELETE ON change_sets
WHEN (SELECT active FROM history_purge_guard WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'change_sets değişmezdir: bir kayıt silinemez, geri alınır');
END;

-- Bir küme (S) silinince, S'yi geri alan/düzelten kümenin `revokes_change_set_id`
-- alanı artık var olmayan bir satırı gösteremez (FK) — NULL'a çekilmelidir.
-- Guard açıkken TEK bu sütunun NULL'a çekilmesine izin verilir; kümenin
-- diğer HİÇBİR alanı (kind/effective_date/reason/actor/recorded_at/
-- impact_json) değişemez — WHEN koşulu bunu satır satır denetler.
DROP TRIGGER trg_change_sets_no_update;
CREATE TRIGGER trg_change_sets_no_update
BEFORE UPDATE ON change_sets
WHEN NOT (
    (SELECT active FROM history_purge_guard WHERE id = 1) = 1
    AND NEW.term = OLD.term
    AND NEW.kind = OLD.kind
    AND NEW.effective_date = OLD.effective_date
    AND (NEW.document_date IS OLD.document_date)
    AND NEW.reason = OLD.reason
    AND NEW.actor = OLD.actor
    AND NEW.recorded_at = OLD.recorded_at
    AND NEW.impact_json = OLD.impact_json
    AND NEW.revokes_change_set_id IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'change_sets değişmezdir: bir kayıt güncellenemez, yenisi eklenir');
END;

-- ============================================================
-- change_events — DELETE guard'a bağlanır
-- ============================================================
DROP TRIGGER trg_change_events_no_delete;
CREATE TRIGGER trg_change_events_no_delete
BEFORE DELETE ON change_events
WHEN (SELECT active FROM history_purge_guard WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'change_events değişmezdir: bir kayıt silinemez, geri alınır');
END;

-- `caused_by` YALNIZ GÖRÜNTÜLEME amaçlıdır (bkz. `domain/history/decide/revoke.rs`
-- yorumu: "salt görüntüleme amaçlıdır"; hiçbir `apply_*`/projeksiyon
-- fonksiyonu okumaz — bkz. `domain/history/audit.rs`, TEK okuyucu). Bir
-- işaret (`revoked`) olayı silinince, onu `caused_by` ile referans alan
-- KALAN bir kayıt (ör. geri almanın tetiklediği kapasite/tavan olayı) artık
-- var olmayan bir satırı gösteremez (FK); guard açıkken TEK bu sütunun
-- NULL'a çekilmesine izin verilir.
DROP TRIGGER trg_change_events_no_update;
CREATE TRIGGER trg_change_events_no_update
BEFORE UPDATE ON change_events
WHEN NOT (
    (SELECT active FROM history_purge_guard WHERE id = 1) = 1
    AND NEW.change_set_id = OLD.change_set_id
    AND NEW.stream = OLD.stream
    AND NEW.subject_id = OLD.subject_id
    AND NEW.term = OLD.term
    AND NEW.kind = OLD.kind
    AND NEW.kind_version = OLD.kind_version
    AND NEW.effective_date = OLD.effective_date
    AND NEW.payload = OLD.payload
    AND (NEW.revokes IS OLD.revokes)
    AND NEW.caused_by IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'change_events değişmezdir: bir kayıt güncellenemez, yenisi eklenir');
END;
