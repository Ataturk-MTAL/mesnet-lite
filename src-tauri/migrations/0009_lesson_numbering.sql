-- Ders saati numaralandırmasını 1'den başlatır (kullanıcı kararı: "Gün
-- Başlangıç Saati" ayarı kalkıyor; ders saatleri artık HER ZAMAN 1'den N'e
-- numaralanıyor — bkz. `db/settings.rs::lesson_hour_bounds`, tek doğruluk
-- kaynağı).
--
-- ESKİ numaralandırma günün gerçek saatiydi: migration 0001'in seed'i
-- "Gün Başlangıç Saati" = 8, "Gün Bitiş Saati" = 17 yazıyordu, yani kayıtlı
-- programlar 8..16 arasında (9 saatlik gün) numaralanmıştı. Kanıt (gerçek
-- veritabanında doğrulandı): `teacher_schedule_periods.slots_json` içindeki
-- saatler min 8, maks 16; `change_events`teki dört `schedule_set` olayının
-- `payload.schedule` dizileri de aynı aralıkta.
--
-- KAYMA SABİTİ = 7 = eski başlangıç (8) - yeni başlangıç (1). Aynı 9 saatlik
-- gün artık 1..9 olarak ifade edilir (8'i 1 yapan kayma, 16'yı 9 yapar).
-- Bu göç, saat taşıyan HER sütun/JSON alanında AYNI kaymayı uygular; hiçbir
-- veri kaybolmaz, yalnız numaralandırma kayar.
--
-- KAPSAM (saat taşıyan TÜM veri):
--   * teacher_schedule_periods.slots_json     — `[[gün, saat], ...]` projeksiyonu
--   * change_events.payload (kind='schedule_set')       — `$.schedule` dizisi
--   * change_events.payload (kind='coordinator_assigned') — `$.state.visitHour`
--   * coordination_periods.visit_hour         — koordinasyon projeksiyonu
--   * assignments.visit_hour                  — atama tahtasının DOĞRUDAN (event-
--                                                sourced olmayan) eski tablosu
-- Bu veritabanında `coordination_periods` ve `assignments` 0 satırdır (henüz
-- hiçbir atama yapılmamış); yine de aynı kaymayı taşırlar, çünkü başka bir
-- kurulumda dolu olabilirler.
--
-- SINIR: 8'den KÜÇÜK bir saat, bu satırın ZATEN yeni (1'den başlayan)
-- numaralandırmayla yazıldığı anlamına gelir (ör. göç ikinci kez çalıştı ya
-- da satır zaten 1..7 aralığındaydı) — böyle bir satıra DOKUNULMAZ. Her
-- UPDATE'in `WHERE ... >= 8` koşulu bunu sağlar; bu aynı zamanda göçü doğal
-- olarak İDEMPOTENT yapar (`_sqlx_migrations` zaten tek seferlik uygulamayı
-- garanti eder, bu ek bir güvenlik katmanıdır — bkz. `migration_0009_tests.rs`
-- deki idempotentlik testi).
--
-- `change_events` NORMALDE değişmezdir (bkz. migration 0006'daki
-- `trg_change_events_no_update` tetikleyicisi, "bir kayıt güncellenemez,
-- yenisi eklenir"). Bu göç İSTİSNAdır: olayın ANLAMI değişmiyor, yalnızca
-- saatin YAZILDIĞI numaralandırma biçimi düzeltiliyor — bu yüzden
-- tetikleyici burada GEÇİCİ olarak kaldırılıp güncellemeden hemen sonra
-- AYNEN geri kurulur; bu göçten sonraki hiçbir yazma bu istisnadan etkilenmez.

PRAGMA foreign_keys = ON;

DROP TRIGGER trg_change_events_no_update;

-- 1) `teacher_schedule_periods.slots_json`: her [gün, saat] çiftinin
--    saat kısmını (>= 8 olanları) 7 azalt, diziyi AYNI sırayla geri yaz.
UPDATE teacher_schedule_periods
SET slots_json = (
    SELECT json_group_array(json_array(
        json_extract(slot.value, '$[0]'),
        CASE WHEN json_extract(slot.value, '$[1]') >= 8
             THEN json_extract(slot.value, '$[1]') - 7
             ELSE json_extract(slot.value, '$[1]')
        END
    ))
    FROM json_each(teacher_schedule_periods.slots_json) AS slot
)
WHERE EXISTS (
    SELECT 1 FROM json_each(teacher_schedule_periods.slots_json) AS slot
    WHERE json_extract(slot.value, '$[1]') >= 8
);

-- 2) `change_events.payload` (kind='schedule_set'): AYNI kaymayı `$.schedule`
--    dizisine uygular; olayın diğer alanları (previousSlotCount, source,
--    labels) dokunulmadan kalır.
UPDATE change_events
SET payload = json_set(
    payload,
    '$.schedule',
    json((
        SELECT json_group_array(json_array(
            json_extract(slot.value, '$[0]'),
            CASE WHEN json_extract(slot.value, '$[1]') >= 8
                 THEN json_extract(slot.value, '$[1]') - 7
                 ELSE json_extract(slot.value, '$[1]')
            END
        ))
        FROM json_each(json_extract(change_events.payload, '$.schedule')) AS slot
    ))
)
WHERE kind = 'schedule_set'
  AND EXISTS (
      SELECT 1 FROM json_each(json_extract(change_events.payload, '$.schedule')) AS slot
      WHERE json_extract(slot.value, '$[1]') >= 8
  );

-- 3) `change_events.payload` (kind='coordinator_assigned'): `$.state.visitHour`
--    tek bir sayıdır, dizi değildir.
UPDATE change_events
SET payload = json_set(payload, '$.state.visitHour', json_extract(payload, '$.state.visitHour') - 7)
WHERE kind = 'coordinator_assigned'
  AND json_extract(payload, '$.state.visitHour') >= 8;

CREATE TRIGGER trg_change_events_no_update
BEFORE UPDATE ON change_events
BEGIN
    SELECT RAISE(ABORT, 'change_events değişmezdir: bir kayıt güncellenemez, yenisi eklenir');
END;

-- 4) `coordination_periods.visit_hour` — koordinasyon projeksiyonu (bu
--    veritabanında 0 satır).
UPDATE coordination_periods
SET visit_hour = visit_hour - 7
WHERE visit_hour >= 8;

-- 5) `assignments.visit_hour` — atama tahtasının doğrudan yazılan eski
--    tablosu (bu veritabanında 0 satır).
UPDATE assignments
SET visit_hour = visit_hour - 7
WHERE visit_hour >= 8;
