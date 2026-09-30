-- Öğretmen unvanına 'principal' (Okul Müdürü) ve 'deputy_principal' (Müdür
-- Yardımcısı) eklenir. `teachers.chief_type` CHECK kısıtı yalnız üç değere
-- izin veriyordu; SQLite'ta CHECK değiştirmenin tek güvenli yolu tabloyu
-- yeniden kurmaktır.
--
-- VERİ KAYBI RİSKİ VE NEDEN BU ŞEKİLDE YAZILDI: `teacher_availability` ve
-- `assignments` `teachers(id)`'ye `ON DELETE CASCADE` ile bağlıdır (0004
-- `assignment_slots`'u kaldırdı; `assignments`'ın altında başka tablo yok) ve uygulama bağlantıları
-- `foreign_keys = ON` ile açılır. `DROP TABLE teachers` örtük bir DELETE
-- yaptığı için bu satırlar SESSİZCE silinir. SQLite belgesinin önerdiği
-- `PRAGMA foreign_keys = OFF` burada işe yaramaz: sqlx-sqlite 0.8 her göçü
-- (`-- no-transaction` yorumuna rağmen) bir işlem içinde çalıştırır ve
-- işlem içindeki bu PRAGMA etkisizdir. Bu yüzden bağlı satırlar önce geçici
-- tablolara kopyalanır, öğretmenler yeniden kurulduktan sonra AYNI kimliklerle
-- geri yazılır ve sayılar tutmazsa göç (tüm işlemle birlikte) geri alınır.
--
-- Tarihçe tabloları (`teacher_load_periods`, `change_events` vb.) `teachers`'a
-- yabancı anahtarla bağlı değildir ve `chief_type` üzerinde CHECK taşımaz
-- (0006); dokunulmaları gerekmez.

CREATE TEMP TABLE saved_teachers      AS SELECT * FROM teachers;
CREATE TEMP TABLE saved_availability  AS SELECT * FROM teacher_availability;
CREATE TEMP TABLE saved_assignments   AS SELECT * FROM assignments;

CREATE TABLE teachers_new (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    first_name         TEXT    NOT NULL,
    last_name          TEXT    NOT NULL,
    registry_no        TEXT    NOT NULL DEFAULT '',
    field              TEXT    NOT NULL,
    branches           TEXT    NOT NULL DEFAULT '[]',
    employment_type    TEXT    NOT NULL DEFAULT 'tenured'
                               CHECK (employment_type IN ('tenured','contracted')),
    base_hours         INTEGER NOT NULL DEFAULT 20,   -- MADDE 5/1-ç
    max_extra_hours    INTEGER NOT NULL DEFAULT 24,   -- MADDE 6/1-c
    other_extra_hours  INTEGER NOT NULL DEFAULT 0,
    chief_type         TEXT    NOT NULL DEFAULT 'none'
                               CHECK (chief_type IN ('none','workshop_lab','department','principal','deputy_principal')),
    is_active          INTEGER NOT NULL DEFAULT 1
);

INSERT INTO teachers_new
    (id, first_name, last_name, registry_no, field, branches, employment_type,
     base_hours, max_extra_hours, other_extra_hours, chief_type, is_active)
SELECT id, first_name, last_name, registry_no, field, branches, employment_type,
       base_hours, max_extra_hours, other_extra_hours, chief_type, is_active
FROM saved_teachers;

-- AUTOINCREMENT sayacı taşınır: en yüksek kimlikli öğretmen silinmişse bile
-- kimlikler yeniden kullanılmaz (tarihçe olayları `subject_id` ile öğretmene
-- bağlıdır; yeniden kullanılan kimlik eski olayları yeni kişiye bağlardı).
DELETE FROM sqlite_sequence WHERE name = 'teachers_new';
INSERT INTO sqlite_sequence (name, seq)
    SELECT 'teachers_new', seq FROM sqlite_sequence WHERE name = 'teachers';

DROP TABLE teachers;
ALTER TABLE teachers_new RENAME TO teachers;

INSERT INTO teacher_availability  SELECT * FROM saved_availability;
INSERT INTO assignments           SELECT * FROM saved_assignments;

-- Emniyet: herhangi bir tablonun satır sayısı göç öncesiyle tutmuyorsa
-- NOT NULL ihlali göçü ve bütün işlemi geri alır.
CREATE TEMP TABLE migration_guard (ok INTEGER NOT NULL);
INSERT INTO migration_guard SELECT CASE
    WHEN (SELECT COUNT(*) FROM teachers)             = (SELECT COUNT(*) FROM saved_teachers)
     AND (SELECT COUNT(*) FROM teacher_availability) = (SELECT COUNT(*) FROM saved_availability)
     AND (SELECT COUNT(*) FROM assignments)          = (SELECT COUNT(*) FROM saved_assignments)
    THEN 1 END;

DROP TABLE migration_guard;
DROP TABLE saved_assignments;
DROP TABLE saved_availability;
DROP TABLE saved_teachers;
