-- Dönem içi değişiklik tarihçesi (Faz A, spec 2026-09-19).
--
-- YALNIZ EKLEME yapar: hiçbir eski tablo silinmez ya da yeniden kurulmaz.
-- Doğruluk kaynağı artık `change_sets` + `change_events` günlüğüdür; beş
-- tarih aralıklı projeksiyon tablosu bu günlükten yeniden kurulabilir
-- (`db/projection.rs::rebuild_*`). Eski tablolar (`students.company_id`,
-- `company_term_hours`, `assignments`, öğretmen yük sütunları,
-- `teacher_availability`) `0007`'ye kadar okunmaya devam eder.
--
-- Tarih doğrulaması her yerde `CHECK (date(x) IS x)` ile yapılır: `= x`
-- kullanılmaz, çünkü SQLite `'garbage'` gibi bozuk bir değeri `=` ile
-- sessizce kabul eder; `date()` fonksiyonu bozuk veya boşluksuz (ör.
-- `'2026-1-3'`) bir tarihte NULL döner ve `IS` karşılaştırması bunu yakalar.

PRAGMA foreign_keys = ON;

-- ============================================================
-- 1. Dönem tarihleri
-- ============================================================

-- `dates_confirmed = 0`: göçten gelen varsayılan tarihler kullanıcı
-- tarafından onaylanmamış sayılır (spec §9 adım 1).
CREATE TABLE terms (
    term            TEXT PRIMARY KEY,
    start_date      TEXT    NOT NULL CHECK (date(start_date) IS start_date),
    end_date        TEXT    NOT NULL CHECK (date(end_date) IS end_date),
    dates_confirmed INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL
) STRICT;

-- ============================================================
-- 2. Olay günlüğü — doğruluk kaynağı
-- ============================================================

-- Bir kullanıcı eylemini (ya da göç tohumunu) temsil eder. `revokes_change_set_id`
-- UNIQUE'tir: bir küme en fazla BİR başka kümeyi geri alabilir/düzeltebilir.
CREATE TABLE change_sets (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    term                  TEXT    NOT NULL REFERENCES terms(term),
    kind                  TEXT    NOT NULL,
    effective_date        TEXT    NOT NULL CHECK (date(effective_date) IS effective_date),
    document_date         TEXT    CHECK (document_date IS NULL OR date(document_date) IS document_date),
    reason                TEXT    NOT NULL DEFAULT '',
    actor                 TEXT    NOT NULL DEFAULT '',
    recorded_at           TEXT    NOT NULL,
    revokes_change_set_id INTEGER UNIQUE REFERENCES change_sets(id),
    impact_json           TEXT    NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_change_sets_term ON change_sets(term);

-- `change_sets` DEĞİŞMEZDİR: denetim listesi güvenilir olsun diye ne
-- güncellenir ne silinir. Bir hatayı düzeltmenin tek yolu yeni bir
-- `correct`/`revoke` kümesi eklemektir (spec §5.4).
CREATE TRIGGER trg_change_sets_no_update
BEFORE UPDATE ON change_sets
BEGIN
    SELECT RAISE(ABORT, 'change_sets değişmezdir: bir kayıt güncellenemez, yenisi eklenir');
END;

CREATE TRIGGER trg_change_sets_no_delete
BEFORE DELETE ON change_sets
BEGIN
    SELECT RAISE(ABORT, 'change_sets değişmezdir: bir kayıt silinemez, geri alınır');
END;

-- Doğruluk kaynağının kendisi. `subject_id`'de FK YOKTUR: bir olay kaydı,
-- öznesi (öğrenci/öğretmen/işletme) silinemeyecek olsa bile satırdan uzun
-- yaşayabilmelidir (spec §4.1). `kind`/`kind_version` çifti
-- `EventPayload::decode`'un TEK upcast noktasına karşılık gelir.
CREATE TABLE change_events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    change_set_id  INTEGER NOT NULL REFERENCES change_sets(id),
    stream         TEXT    NOT NULL,
    subject_id     INTEGER NOT NULL,
    term           TEXT    NOT NULL REFERENCES terms(term),
    kind           TEXT    NOT NULL,
    kind_version   INTEGER NOT NULL,
    effective_date TEXT    NOT NULL CHECK (date(effective_date) IS effective_date),
    payload        TEXT    NOT NULL,
    caused_by      INTEGER REFERENCES change_events(id),
    revokes        INTEGER UNIQUE REFERENCES change_events(id),
    -- `kind = 'revoked'` her zaman `revokes` dolu demektir, ve tersi de.
    CHECK ((kind = 'revoked') = (revokes IS NOT NULL))
);

CREATE INDEX idx_change_events_change_set ON change_events(change_set_id);
CREATE INDEX idx_change_events_stream_subject_term ON change_events(stream, subject_id, term);

-- `change_events` de DEĞİŞMEZDİR — aynı gerekçeyle.
CREATE TRIGGER trg_change_events_no_update
BEFORE UPDATE ON change_events
BEGIN
    SELECT RAISE(ABORT, 'change_events değişmezdir: bir kayıt güncellenemez, yenisi eklenir');
END;

CREATE TRIGGER trg_change_events_no_delete
BEFORE DELETE ON change_events
BEGIN
    SELECT RAISE(ABORT, 'change_events değişmezdir: bir kayıt silinemez, geri alınır');
END;

-- ============================================================
-- 3. Beş tarih aralıklı projeksiyon — tek yazıcısı db/projection.rs
-- ============================================================
--
-- Hepsinde ortak olan: `source_event_id`, açık satır için kısmi unique
-- index (`WHERE valid_to IS NULL`), `UNIQUE(özne, term, valid_from)`,
-- `valid_to > valid_from` kontrolü, ve tarih aralığı örtüşen bir satırı
-- reddeden bir `BEFORE INSERT` tetikleyicisi (spec §4.1, "çakışma yedeği").
-- Aralıklar yarı açıktır: `[valid_from, valid_to)`.

-- `placement` akışı: öğrencinin o an bulunduğu işletme.
CREATE TABLE student_placements (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    student_id      INTEGER NOT NULL,
    term            TEXT    NOT NULL REFERENCES terms(term),
    company_id      INTEGER NOT NULL,
    valid_from      TEXT    NOT NULL CHECK (date(valid_from) IS valid_from),
    valid_to        TEXT    CHECK (valid_to IS NULL OR date(valid_to) IS valid_to),
    source_event_id INTEGER NOT NULL REFERENCES change_events(id),
    CHECK (valid_to IS NULL OR valid_to > valid_from),
    UNIQUE (student_id, term, valid_from)
);

CREATE UNIQUE INDEX idx_student_placements_open
    ON student_placements(student_id, term) WHERE valid_to IS NULL;

CREATE TRIGGER trg_student_placements_overlap
BEFORE INSERT ON student_placements
WHEN EXISTS (
    SELECT 1 FROM student_placements
    WHERE student_id = NEW.student_id
      AND term = NEW.term
      AND NEW.valid_from < COALESCE(valid_to, '9999-12-31')
      AND valid_from < COALESCE(NEW.valid_to, '9999-12-31')
)
BEGIN
    SELECT RAISE(ABORT, 'Tarih aralıkları çakışıyor');
END;

-- `company_hours` akışı: özne işletmedir (spec §4.2 tablo).
CREATE TABLE company_hour_periods (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id         INTEGER NOT NULL,
    term               TEXT    NOT NULL REFERENCES terms(term),
    valid_from         TEXT    NOT NULL CHECK (date(valid_from) IS valid_from),
    valid_to           TEXT    CHECK (valid_to IS NULL OR date(valid_to) IS valid_to),
    awarded_hours      INTEGER NOT NULL,
    max_hours_snapshot INTEGER NOT NULL,
    is_honorary        INTEGER NOT NULL DEFAULT 0,
    is_locked          INTEGER NOT NULL DEFAULT 0,
    notes              TEXT    NOT NULL DEFAULT '',
    source_event_id    INTEGER NOT NULL REFERENCES change_events(id),
    CHECK (valid_to IS NULL OR valid_to > valid_from),
    UNIQUE (company_id, term, valid_from)
);

CREATE UNIQUE INDEX idx_company_hour_periods_open
    ON company_hour_periods(company_id, term) WHERE valid_to IS NULL;

CREATE TRIGGER trg_company_hour_periods_overlap
BEFORE INSERT ON company_hour_periods
WHEN EXISTS (
    SELECT 1 FROM company_hour_periods
    WHERE company_id = NEW.company_id
      AND term = NEW.term
      AND NEW.valid_from < COALESCE(valid_to, '9999-12-31')
      AND valid_from < COALESCE(NEW.valid_to, '9999-12-31')
)
BEGIN
    SELECT RAISE(ABORT, 'Tarih aralıkları çakışıyor');
END;

-- `coordination` akışı: özne işletmedir; koordinatör öğretmen bir alandır.
CREATE TABLE coordination_periods (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id       INTEGER NOT NULL,
    term             TEXT    NOT NULL REFERENCES terms(term),
    valid_from       TEXT    NOT NULL CHECK (date(valid_from) IS valid_from),
    valid_to         TEXT    CHECK (valid_to IS NULL OR date(valid_to) IS valid_to),
    teacher_id       INTEGER NOT NULL,
    visit_day        INTEGER NOT NULL,
    visit_hour       INTEGER NOT NULL,
    is_forced        INTEGER NOT NULL DEFAULT 0,
    force_reason     TEXT,
    source_event_id  INTEGER NOT NULL REFERENCES change_events(id),
    CHECK (valid_to IS NULL OR valid_to > valid_from),
    UNIQUE (company_id, term, valid_from)
);

CREATE UNIQUE INDEX idx_coordination_periods_open
    ON coordination_periods(company_id, term) WHERE valid_to IS NULL;

CREATE TRIGGER trg_coordination_periods_overlap
BEFORE INSERT ON coordination_periods
WHEN EXISTS (
    SELECT 1 FROM coordination_periods
    WHERE company_id = NEW.company_id
      AND term = NEW.term
      AND NEW.valid_from < COALESCE(valid_to, '9999-12-31')
      AND valid_from < COALESCE(NEW.valid_to, '9999-12-31')
)
BEGIN
    SELECT RAISE(ABORT, 'Tarih aralıkları çakışıyor');
END;

-- `teacher_load` akışı: okuldaki ders saati, şeflik, kadro durumu.
CREATE TABLE teacher_load_periods (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    teacher_id         INTEGER NOT NULL,
    term               TEXT    NOT NULL REFERENCES terms(term),
    valid_from         TEXT    NOT NULL CHECK (date(valid_from) IS valid_from),
    valid_to           TEXT    CHECK (valid_to IS NULL OR date(valid_to) IS valid_to),
    base_hours         INTEGER NOT NULL,
    max_extra_hours    INTEGER NOT NULL,
    other_extra_hours  INTEGER NOT NULL,
    chief_type         TEXT    NOT NULL,
    employment_type    TEXT    NOT NULL,
    source_event_id    INTEGER NOT NULL REFERENCES change_events(id),
    CHECK (valid_to IS NULL OR valid_to > valid_from),
    UNIQUE (teacher_id, term, valid_from)
);

CREATE UNIQUE INDEX idx_teacher_load_periods_open
    ON teacher_load_periods(teacher_id, term) WHERE valid_to IS NULL;

CREATE TRIGGER trg_teacher_load_periods_overlap
BEFORE INSERT ON teacher_load_periods
WHEN EXISTS (
    SELECT 1 FROM teacher_load_periods
    WHERE teacher_id = NEW.teacher_id
      AND term = NEW.term
      AND NEW.valid_from < COALESCE(valid_to, '9999-12-31')
      AND valid_from < COALESCE(NEW.valid_to, '9999-12-31')
)
BEGIN
    SELECT RAISE(ABORT, 'Tarih aralıkları çakışıyor');
END;

-- `teacher_schedule` akışı: boş saatlerin JSON çift dizisi
-- (`[[gün, saat], ...]`) — `EventPayload::ScheduleSet` ile aynı biçim.
CREATE TABLE teacher_schedule_periods (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    teacher_id       INTEGER NOT NULL,
    term             TEXT    NOT NULL REFERENCES terms(term),
    valid_from       TEXT    NOT NULL CHECK (date(valid_from) IS valid_from),
    valid_to         TEXT    CHECK (valid_to IS NULL OR date(valid_to) IS valid_to),
    slots_json       TEXT    NOT NULL DEFAULT '[]',
    source_event_id  INTEGER NOT NULL REFERENCES change_events(id),
    CHECK (valid_to IS NULL OR valid_to > valid_from),
    UNIQUE (teacher_id, term, valid_from)
);

CREATE UNIQUE INDEX idx_teacher_schedule_periods_open
    ON teacher_schedule_periods(teacher_id, term) WHERE valid_to IS NULL;

CREATE TRIGGER trg_teacher_schedule_periods_overlap
BEFORE INSERT ON teacher_schedule_periods
WHEN EXISTS (
    SELECT 1 FROM teacher_schedule_periods
    WHERE teacher_id = NEW.teacher_id
      AND term = NEW.term
      AND NEW.valid_from < COALESCE(valid_to, '9999-12-31')
      AND valid_from < COALESCE(NEW.valid_to, '9999-12-31')
)
BEGIN
    SELECT RAISE(ABORT, 'Tarih aralıkları çakışıyor');
END;

-- ============================================================
-- 4. Küçük şema ekleri
-- ============================================================

-- Yumuşak silme: geçmişi olan bir işletme veritabanından SİLİNEMEZ
-- (spec §5.4), yerine pasif yapılır. Mevcut işletmeler aktif başlar.
ALTER TABLE companies ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1;

-- `change_sets.actor` buradan okunur (R4, aynı bağlantı üzerinden).
INSERT OR IGNORE INTO settings (key, value) VALUES ('operator_name', '');

-- ============================================================
-- 5. Göç tohumu (spec §9)
-- ============================================================

-- Adım 1: `terms` satırları — bugünkü `known_terms` birleşiminden (bkz.
-- `db/settings.rs::known_terms`, aynı yedi kaynağın birleşimi).
-- Biçimi bozuk bir dönem adı ("YYYY-YYYY/N" değilse ya da ikinci yıl
-- birinciden bir fazla değilse) `TermDates::default_for`'un "bozuk"
-- kolunu izler: 2000-01-01–2099-12-31. `CAST(.. AS INTEGER)` SQLite'ta
-- ayrıştırılamayan metinde hata vermez, 0 döner — bu yüzden `well_formed`
-- bayrağı yanlışken year1/year2 değerleri güvenle yok sayılabilir.
INSERT INTO terms (term, start_date, end_date, dates_confirmed, created_at)
SELECT
    kt.term,
    CASE
        WHEN kt.well_formed = 1 AND kt.half = 1 THEN kt.year1_text || '-09-01'
        WHEN kt.well_formed = 1 AND kt.half = 2 THEN kt.year2_text || '-02-01'
        ELSE '2000-01-01'
    END,
    CASE
        WHEN kt.well_formed = 1 AND kt.half = 1 THEN kt.year2_text || '-01-31'
        WHEN kt.well_formed = 1 AND kt.half = 2 THEN kt.year2_text || '-06-30'
        ELSE '2099-12-31'
    END,
    0,
    datetime('now')
FROM (
    SELECT
        term,
        CASE
            WHEN term GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9][0-9][0-9]/[12]'
                 AND CAST(substr(term, 6, 4) AS INTEGER) = CAST(substr(term, 1, 4) AS INTEGER) + 1
            THEN 1 ELSE 0
        END AS well_formed,
        substr(term, 1, 4)  AS year1_text,
        substr(term, 6, 4)  AS year2_text,
        CAST(substr(term, 11, 1) AS INTEGER) AS half
    FROM (
        SELECT term FROM students WHERE term <> ''
        UNION SELECT term FROM teacher_availability WHERE term <> ''
        UNION SELECT term FROM class_workplace_days WHERE term <> ''
        UNION SELECT term FROM company_term_hours WHERE term <> ''
        UNION SELECT term FROM assignments WHERE term <> ''
        UNION SELECT term FROM term_branch_hours WHERE term <> ''
        UNION SELECT value FROM settings WHERE key = 'active_term' AND value <> ''
    )
) AS kt;

-- Adım 2: her dönem için bir `opening` değişiklik kümesi.
INSERT INTO change_sets (term, kind, effective_date, document_date, reason, actor, recorded_at, revokes_change_set_id, impact_json)
SELECT
    term,
    'opening',
    start_date,
    NULL,
    'Göç: 0005 şemasındaki durumun açılış anlık görüntüsü',
    'migration',
    datetime('now'),
    NULL,
    '{}'
FROM terms;

-- Adım 3a: `students.company_id` → `student_placed` (placement akışı,
-- özne öğrenci). İşletmesi olmayan öğrenci hiç olay almaz (durum: yok).
INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
SELECT
    cs.id,
    'placement',
    s.id,
    s.term,
    'student_placed',
    1,
    t.start_date,
    json_object(
        'toCompanyId', c.id,
        'fromCompanyId', NULL,
        'source', 'opening',
        'labels', json_object(
            'student', s.first_name || ' ' || s.last_name,
            'toCompany', c.name
        )
    ),
    NULL,
    NULL
FROM students s
JOIN companies c ON c.id = s.company_id
JOIN terms t ON t.term = s.term
JOIN change_sets cs ON cs.term = s.term AND cs.kind = 'opening';

-- Adım 3b: `company_term_hours` → `hours_set` (company_hours akışı,
-- özne işletme).
INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
SELECT
    cs.id,
    'company_hours',
    h.company_id,
    h.term,
    'hours_set',
    1,
    t.start_date,
    json_object(
        'state', json_object(
            'awardedHours', h.awarded_hours,
            'maxHoursSnapshot', h.max_hours_snapshot,
            'isHonorary', iif(h.is_honorary = 1, json('true'), json('false')),
            'isLocked', iif(h.is_locked = 1, json('true'), json('false')),
            'notes', h.notes
        ),
        'previousAwarded', NULL,
        'labels', json_object('company', c.name)
    ),
    NULL,
    NULL
FROM company_term_hours h
JOIN companies c ON c.id = h.company_id
JOIN terms t ON t.term = h.term
JOIN change_sets cs ON cs.term = h.term AND cs.kind = 'opening';

-- Adım 3c: `assignments` → `coordinator_assigned` (coordination akışı,
-- özne işletme; koordinatör bir alandır).
INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
SELECT
    cs.id,
    'coordination',
    a.company_id,
    a.term,
    'coordinator_assigned',
    1,
    t.start_date,
    json_object(
        'state', json_object(
            'teacherId', a.teacher_id,
            'visitDay', a.visit_day,
            'visitHour', a.visit_hour,
            'isForced', iif(a.is_forced = 1, json('true'), json('false')),
            'forceReason', a.force_reason
        ),
        'fromTeacherId', NULL,
        'labels', json_object('company', c.name, 'teacher', tc.first_name || ' ' || tc.last_name)
    ),
    NULL,
    NULL
FROM assignments a
JOIN companies c ON c.id = a.company_id
JOIN teachers tc ON tc.id = a.teacher_id
JOIN terms t ON t.term = a.term
JOIN change_sets cs ON cs.term = a.term AND cs.kind = 'opening';

-- Adım 3d: öğretmen yük sütunları → `load_set` (teacher_load akışı,
-- özne öğretmen). Eski şemada yük DÖNEME BAĞLI DEĞİLDİ (`teachers`
-- tablosunda tek satır); bu yüzden HER bilinen dönem × HER öğretmen için
-- bugünkü tek değer tohumlanır. Aktif dönem dışındakiler `source =
-- 'opening_assumed'` işaretlenir: geçmiş dönemlerde yükün gerçekten aynı
-- olduğu VARSAYILIR, kanıtlanmış değildir (spec §9 adım 3).
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
        'source', CASE
            WHEN t.term = COALESCE((SELECT value FROM settings WHERE key = 'active_term'), '')
            THEN 'opening' ELSE 'opening_assumed'
        END,
        'labels', json_object('teacher', tc.first_name || ' ' || tc.last_name)
    ),
    NULL,
    NULL
FROM terms t
CROSS JOIN teachers tc
JOIN change_sets cs ON cs.term = t.term AND cs.kind = 'opening';

-- Adım 3e: `teacher_availability` → `schedule_set` (teacher_schedule
-- akışı, özne öğretmen). `json_group_array` çıktısı `ORDER BY
-- day_of_week, hour` ile sıralanır: `WeeklySchedule`'ın `BTreeSet<Slot>`
-- katlaması aynı sırayı üretir, aksi hâlde `encode()` çıktısıyla yalnız
-- KÜME olarak eşleşir, DİZİ olarak (JSON değer karşılaştırmasında konum
-- önemlidir) eşleşmeyebilirdi.
INSERT INTO change_events (change_set_id, stream, subject_id, term, kind, kind_version, effective_date, payload, caused_by, revokes)
SELECT
    cs.id,
    'teacher_schedule',
    ta.teacher_id,
    ta.term,
    'schedule_set',
    1,
    t.start_date,
    json_object(
        'schedule', (
            SELECT json_group_array(json_array(ta2.day_of_week, ta2.hour))
            FROM teacher_availability ta2
            WHERE ta2.teacher_id = ta.teacher_id AND ta2.term = ta.term
            ORDER BY ta2.day_of_week, ta2.hour
        ),
        'previousSlotCount', NULL,
        'source', 'opening',
        'labels', json_object('teacher', tc.first_name || ' ' || tc.last_name)
    ),
    NULL,
    NULL
FROM (SELECT DISTINCT teacher_id, term FROM teacher_availability) ta
JOIN teachers tc ON tc.id = ta.teacher_id
JOIN terms t ON t.term = ta.term
JOIN change_sets cs ON cs.term = ta.term AND cs.kind = 'opening';

-- Adım 4: Projeksiyonlar — her açılış olayından `[start_date, NULL)`
-- aralıklı TEK bir satır. `json_extract` bir JSON true/false'u SQL
-- 1/0'a çevirir (SQLite belgeleri), bu yüzden INTEGER sütunlara doğrudan
-- yazılabilir.
INSERT INTO student_placements (student_id, term, company_id, valid_from, valid_to, source_event_id)
SELECT ce.subject_id, ce.term, json_extract(ce.payload, '$.toCompanyId'), ce.effective_date, NULL, ce.id
FROM change_events ce
JOIN change_sets cs ON cs.id = ce.change_set_id AND cs.kind = 'opening'
WHERE ce.kind = 'student_placed';

INSERT INTO company_hour_periods
    (company_id, term, valid_from, valid_to, awarded_hours, max_hours_snapshot, is_honorary, is_locked, notes, source_event_id)
SELECT
    ce.subject_id, ce.term, ce.effective_date, NULL,
    json_extract(ce.payload, '$.state.awardedHours'),
    json_extract(ce.payload, '$.state.maxHoursSnapshot'),
    json_extract(ce.payload, '$.state.isHonorary'),
    json_extract(ce.payload, '$.state.isLocked'),
    json_extract(ce.payload, '$.state.notes'),
    ce.id
FROM change_events ce
JOIN change_sets cs ON cs.id = ce.change_set_id AND cs.kind = 'opening'
WHERE ce.kind = 'hours_set';

INSERT INTO coordination_periods
    (company_id, term, valid_from, valid_to, teacher_id, visit_day, visit_hour, is_forced, force_reason, source_event_id)
SELECT
    ce.subject_id, ce.term, ce.effective_date, NULL,
    json_extract(ce.payload, '$.state.teacherId'),
    json_extract(ce.payload, '$.state.visitDay'),
    json_extract(ce.payload, '$.state.visitHour'),
    json_extract(ce.payload, '$.state.isForced'),
    json_extract(ce.payload, '$.state.forceReason'),
    ce.id
FROM change_events ce
JOIN change_sets cs ON cs.id = ce.change_set_id AND cs.kind = 'opening'
WHERE ce.kind = 'coordinator_assigned';

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
JOIN change_sets cs ON cs.id = ce.change_set_id AND cs.kind = 'opening'
WHERE ce.kind = 'load_set';

INSERT INTO teacher_schedule_periods (teacher_id, term, valid_from, valid_to, slots_json, source_event_id)
SELECT ce.subject_id, ce.term, ce.effective_date, NULL, json_extract(ce.payload, '$.schedule'), ce.id
FROM change_events ce
JOIN change_sets cs ON cs.id = ce.change_set_id AND cs.kind = 'opening'
WHERE ce.kind = 'schedule_set';
