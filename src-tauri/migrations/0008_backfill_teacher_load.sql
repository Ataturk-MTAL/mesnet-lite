-- Geriye dönük şeflik/yük tohumu (spec R5c takip düzeltmesi).
--
-- MADDE 6/4 ve OÖKY MADDE 88/2-ç'ye göre havuzun şeflik payı
-- `teacher_load_periods` projeksiyonundan okunur (bkz.
-- db/teaching_load.rs::chief_planning_hours). Göç 0006 yalnız O ANDA VAR
-- OLAN öğretmen(ler) için açılış olayı yazdı; 0006'dan SONRA
-- `commands/teacher_commands.rs::create_teacher` üzerinden eklenen
-- öğretmenler (eski, `execute_change`e hiç uğramayan yol) hiçbir olay
-- almadı ve projeksiyonda satırları yok — havuz onları göremiyordu (kanıtlı
-- örnek: 12 öğretmenden yalnızca biri projeksiyonda, şeflik saatleri 6 yerine
-- 64 olması gerekirken 6 görünüyordu).
--
-- Bu göç, 0006'nın Adım 3d/4'ündeki AYNI deseni (mevcut 'opening' değişiklik
-- kümesine yeni bir `load_set` olayı ekle, sonra projeksiyona
-- [dönem_başı, NULL) satırı yaz) izler; TEK fark, yalnız projeksiyonda
-- satırı OLMAYAN (öğretmen, dönem) çiftlerini işlemesidir — 0006'nın zaten
-- doldurduğu çiftlere ikinci kez yazıp UNIQUE kısıtına çarpmamak için.
--
-- İkinci adım ayrıca `NOT EXISTS` ile korunur (0006'da yok): bu göç birden
-- fazla kez (ör. testte) uygulanırsa ilk uygulamanın yazdığı satırlar ikinci
-- uygulamada tekrar seçilip çakışma hatası vermesin diye.

PRAGMA foreign_keys = ON;

INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
SELECT
    cs.id,
    'teacher_load',
    tc.id,
    t.term,
    'load_set',
    1,
    t.start_date,
    json_object(
        'load', json_object(
            'baseHours', tc.base_hours,
            'maxExtraHours', tc.max_extra_hours,
            'otherExtraHours', tc.other_extra_hours,
            'chiefType', tc.chief_type,
            'employmentType', tc.employment_type
        ),
        'previous', NULL,
        'source', 'backfill_0008',
        'labels', json_object('teacher', tc.first_name || ' ' || tc.last_name)
    ),
    NULL,
    NULL
FROM terms t
CROSS JOIN teachers tc
JOIN change_sets cs ON cs.term = t.term AND cs.kind = 'opening'
WHERE NOT EXISTS (
    SELECT 1 FROM teacher_load_periods p
    WHERE p.teacher_id = tc.id AND p.term = t.term
);

INSERT INTO teacher_load_periods
    (teacher_id, term, valid_from, valid_to, base_hours, max_extra_hours, other_extra_hours, chief_type, employment_type, source_event_id)
SELECT
    ce.subject_id, ce.term, ce.effective_date, NULL,
    json_extract(ce.payload, '$.load.baseHours'),
    json_extract(ce.payload, '$.load.maxExtraHours'),
    json_extract(ce.payload, '$.load.otherExtraHours'),
    json_extract(ce.payload, '$.load.chiefType'),
    json_extract(ce.payload, '$.load.employmentType'),
    ce.id
FROM change_events ce
WHERE ce.kind = 'load_set'
  AND json_extract(ce.payload, '$.source') = 'backfill_0008'
  AND NOT EXISTS (
      SELECT 1 FROM teacher_load_periods p
      WHERE p.teacher_id = ce.subject_id AND p.term = ce.term
  );
