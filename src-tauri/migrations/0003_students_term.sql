-- Öğrenciler eğitim-öğretim yılına bağlanır.
--
-- Her yıl öğrenci listesi baştan gelir; işletmeler ise veritabanında kalıcıdır
-- ve yıllar arasında yeniden kullanılır. Bu yüzden `term` yalnızca `students`
-- tablosuna eklenir, `companies` tablosuna eklenmez.
--
-- Mevcut kayıtlar aktif döneme atanır; tablo boşsa bu güncelleme etkisizdir.

ALTER TABLE students ADD COLUMN term TEXT NOT NULL DEFAULT '';

UPDATE students
SET term = COALESCE((SELECT value FROM settings WHERE key = 'active_term'), '')
WHERE term = '';

CREATE INDEX idx_students_term ON students(term);

-- Aynı dönemde aynı öğrenci numarası iki kez girilemez.
-- SQLite NULL değerleri birbirinden farklı sayar, bu yüzden numarası olmayan
-- birden çok öğrenci bu kısıtı ihlal etmez.
CREATE UNIQUE INDEX idx_students_no_per_term ON students(student_no, term)
WHERE student_no IS NOT NULL;
