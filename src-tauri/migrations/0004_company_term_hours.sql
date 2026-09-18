-- Saat takdiri atamadan AYRILIYOR.
--
-- Önceki model: saat `assignments.awarded_hours` içindeydi, yani bir işletmeye
-- saat yazmak için önce öğretmen atamak gerekiyordu ve yerleştirilen dilim sayısı
-- saate eşit olmak zorundaydı.
--
-- Yeni model (MESNET ile aynı):
--   1. `company_term_hours` — her işletmeye dönem başına takdir edilen saat.
--      Atamadan ÖNCE yapılır; toplam, okulun ders yükü havuzuyla karşılaştırılır.
--   2. `assignments` — işletmeyi bir öğretmenin haftalık programında TEK bir
--      (gün, ders saati) hücresine yerleştirir. Saat burada tutulmaz.
--
-- Ziyaretin yeri ile ek ders saati ayrı şeylerdir: koordinatör 3. ders saatinde
-- gider, o işletme için 6 saat ek ders tahakkuk eder.

-- İşletmenin dönemlik saat takdiri
CREATE TABLE company_term_hours (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    company_id         INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    term               TEXT    NOT NULL,
    -- Kural tablosundan hesaplanan tavan, takdir anında dondurulur.
    -- Kural tablosu sonradan değişse de imzalanmış çizelge değişmemelidir.
    max_hours_snapshot INTEGER NOT NULL,
    -- Takdir edilen saat. Fahri ziyarette 0'dır.
    awarded_hours      INTEGER NOT NULL,
    -- Fahri ziyaret: öğretmen gider, ek ders ücreti doğmaz.
    is_honorary        INTEGER NOT NULL DEFAULT 0,
    -- Kilitli satırlar otomatik dağıtımda korunur.
    is_locked          INTEGER NOT NULL DEFAULT 0,
    notes              TEXT    NOT NULL DEFAULT '',
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL,
    UNIQUE (company_id, term)
);

CREATE INDEX idx_company_term_hours_term ON company_term_hours(term);

-- Atamalar yeniden kuruluyor: saat sütunları kalkıyor, ziyaret hücresi geliyor.
-- Eski tabloda veri yok (atama ekranı henüz yazılmadı), bu yüzden taşıma gerekmez.
DROP TABLE IF EXISTS assignment_slots;
DROP TABLE IF EXISTS assignments;

CREATE TABLE assignments (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    teacher_id     INTEGER NOT NULL REFERENCES teachers(id) ON DELETE CASCADE,
    company_id     INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    term           TEXT    NOT NULL,
    -- Ziyaretin yeri: 1 = Pazartesi … 5 = Cuma, ve kaçıncı ders saati.
    visit_day      INTEGER NOT NULL CHECK (visit_day BETWEEN 1 AND 5),
    visit_hour     INTEGER NOT NULL,
    -- Zorlama: kural dışına bilerek çıkıldığında gerekçesiyle kaydedilir.
    is_forced      INTEGER NOT NULL DEFAULT 0,
    force_reason   TEXT,
    created_at     TEXT    NOT NULL,
    updated_at     TEXT    NOT NULL,
    -- Bir işletme dönem başına tek koordinatöre bağlanır.
    UNIQUE (company_id, term),
    -- Aynı öğretmenin aynı hücresine iki işletme konulamaz.
    UNIQUE (teacher_id, term, visit_day, visit_hour)
);

CREATE INDEX idx_assignments_term ON assignments(term);
CREATE INDEX idx_assignments_teacher ON assignments(teacher_id, term);
