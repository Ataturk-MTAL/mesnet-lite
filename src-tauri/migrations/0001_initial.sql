-- MESNET.Lite başlangıç şeması.
-- Gün numaralandırması: 1 = Pazartesi … 5 = Cuma
-- Saatler tam saat tamsayısıdır (9 = 09:00-10:00); işletme ders saati 60 dakikadır.

PRAGMA foreign_keys = ON;

-- İşletmeler
CREATE TABLE companies (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL,
    contact_first_name  TEXT    NOT NULL DEFAULT '',
    contact_last_name   TEXT    NOT NULL DEFAULT '',
    phone               TEXT    NOT NULL DEFAULT '',
    email               TEXT    NOT NULL DEFAULT '',
    address_text        TEXT    NOT NULL,
    latitude            REAL,
    longitude           REAL,
    geocode_status      TEXT    NOT NULL DEFAULT 'pending'
                                CHECK (geocode_status IN ('pending','resolved','failed','manual')),
    -- TEK YÖN yol mesafesi. Saat kurallarında iki katı kullanılır; iki katı saklanmaz.
    -- Bu değer koordinatlardan HESAPLANMAZ: haversine kuş uçuşu verir, araç yoldan gider.
    -- CSV'den gelir veya elle girilir.
    one_way_distance_km REAL,
    notes               TEXT    NOT NULL DEFAULT '',
    created_at          TEXT    NOT NULL,
    updated_at          TEXT    NOT NULL
);

-- Öğrenciler
CREATE TABLE students (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    first_name   TEXT    NOT NULL,
    last_name    TEXT    NOT NULL,
    student_no   TEXT,
    grade        TEXT    NOT NULL,
    branch       TEXT    NOT NULL,
    company_id   INTEGER REFERENCES companies(id) ON DELETE SET NULL,
    submitted_at TEXT
);

CREATE INDEX idx_students_company ON students(company_id);

-- Öğretmenler
CREATE TABLE teachers (
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
                               CHECK (chief_type IN ('none','workshop_lab','department')),
    is_active          INTEGER NOT NULL DEFAULT 1
);

-- Öğretmenlerin boş saatleri. Satır varsa o saat BOŞ demektir.
CREATE TABLE teacher_availability (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    teacher_id  INTEGER NOT NULL REFERENCES teachers(id) ON DELETE CASCADE,
    day_of_week INTEGER NOT NULL CHECK (day_of_week BETWEEN 1 AND 5),
    hour        INTEGER NOT NULL,
    term        TEXT    NOT NULL,
    UNIQUE (teacher_id, day_of_week, hour, term)
);

-- Sınıfların işletmede bulunduğu günler
CREATE TABLE class_workplace_days (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    grade       TEXT    NOT NULL,
    day_of_week INTEGER NOT NULL CHECK (day_of_week BETWEEN 1 AND 5),
    term        TEXT    NOT NULL,
    UNIQUE (grade, day_of_week, term)
);

-- İşletme saat tavanı kuralları.
-- min_distance_km / max_distance_km GİDİŞ-DÖNÜŞ km'dir (tek yönün iki katı).
CREATE TABLE company_hour_rules (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    min_distance_km REAL    NOT NULL,
    max_distance_km REAL,               -- NULL = üst sınırsız
    min_students    INTEGER NOT NULL,
    max_students    INTEGER,            -- NULL = üst sınırsız
    max_hours       INTEGER NOT NULL
);

-- Atamalar. Bir işletme dönem başına tek koordinatöre bağlanır.
CREATE TABLE assignments (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    teacher_id         INTEGER NOT NULL REFERENCES teachers(id) ON DELETE CASCADE,
    company_id         INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    awarded_hours      INTEGER NOT NULL,
    max_hours_snapshot INTEGER NOT NULL,
    is_forced          INTEGER NOT NULL DEFAULT 0,
    force_reason       TEXT,
    term               TEXT    NOT NULL,
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL,
    UNIQUE (company_id, term)
);

-- Atamanın haftalık ziyaret dilimleri. Her satır bir saattir.
CREATE TABLE assignment_slots (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    assignment_id INTEGER NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    day_of_week   INTEGER NOT NULL CHECK (day_of_week BETWEEN 1 AND 5),
    hour          INTEGER NOT NULL,
    UNIQUE (assignment_id, day_of_week, hour)
);

-- Anahtar/değer ayarlar
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO settings (key, value) VALUES
    ('school_name',              'Atatürk Mesleki ve Teknik Anadolu Lisesi'),
    ('school_latitude',          ''),
    ('school_longitude',         ''),
    ('institution_type',         'other'),
    ('is_metropolitan_district', 'true'),
    ('active_term',              '2026-2027/1'),
    ('day_start_hour',           '8'),
    ('day_end_hour',             '17'),
    ('branch_weekly_hours',      '{}'),
    ('branch_group_counts',      '{}');
