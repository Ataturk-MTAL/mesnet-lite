-- Ders yükü havuzu artık iki bağımsız JSON ayarından değil, gerçek ve
-- döneme bağlı bir tablodan hesaplanıyor.
--
-- Eski model: `settings.branch_weekly_hours` ve `settings.branch_group_counts`
-- adlı iki ayrı JSON, `"{sınıf}|{dal}"` anahtarıyla eşleşmesi gereken iki ayrı
-- harita tutuyordu (bkz. eski hours_commands.rs::compute_pool_hours). Anahtar
-- iki JSON'da da eşleşmezse satır SESSİZCE düşüyordu; ayrıca `settings` tablosu
-- düz `key TEXT PRIMARY KEY` olduğu için havuz döneme bağlı DEĞİLDİ. Grup
-- sayısı her eğitim-öğretim yılında öğrenci sayısıyla değiştiği hâlde, dönem
-- değiştirince geçen yılın grup sayıları kalıyor ve ek ders saati tavanı
-- yanlış çıkıyordu.
--
-- Yeni model: haftalık ders saati ile grup sayısı TEK satırda durur.
-- UNIQUE (term, grade, branch) sayesinde eski tasarımdaki anahtar
-- eşleşmezliği yapısal olarak imkânsız hâle gelir, ve `term` sütunu havuzu
-- doğru döneme bağlar.
CREATE TABLE term_branch_hours (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Eğitim-öğretim yılı ve dönemi, ör. '2026-2027/1'.
    term         TEXT    NOT NULL,
    -- Sınıf, ör. '12/C'.
    grade        TEXT    NOT NULL,
    -- Dal adı, ör. 'Elektronik Haberleşme'.
    branch       TEXT    NOT NULL,
    -- Sınıf+dalın haftalık ders saati (haftalık program saatidir, ek ders değil).
    weekly_hours INTEGER NOT NULL,
    -- O sınıf+dal için açılan grup sayısı; öğrenci sayısına göre yıldan yıla değişir.
    group_count  INTEGER NOT NULL,
    created_at   TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    UNIQUE (term, grade, branch)
);

CREATE INDEX idx_term_branch_hours_term ON term_branch_hours(term);

-- Mevcut ayarları satırlara taşı. Eski ayarlar döneme bağlı değildi; taşıma
-- anında aktif olan dönem hedef alınır, çünkü başka bir dönem varsaymanın
-- hiçbir dayanağı yoktur. Anahtar yalnızca bir JSON'da varsa (eşleşmeyen
-- {sınıf}|{dal}) satır atlanır — eski `filter_map` davranışıyla aynı, ama
-- artık kalıcı bir tabloda kalıcı olarak ifade ediliyor.
--
-- json_each ile iki JSON nesnesinin anahtarları eşlenir; '|' ayracından önceki
-- kısım sınıf, sonraki kısım dal olur (bkz. eski anahtar biçimi
-- "12/C|Elektronik Haberleşme").
INSERT INTO term_branch_hours (term, grade, branch, weekly_hours, group_count, created_at, updated_at)
SELECT
    COALESCE((SELECT value FROM settings WHERE key = 'active_term'), ''),
    substr(w.key, 1, instr(w.key, '|') - 1),
    substr(w.key, instr(w.key, '|') + 1),
    CAST(w.value AS INTEGER),
    CAST(g.value AS INTEGER),
    datetime('now'),
    datetime('now')
FROM json_each(COALESCE((SELECT value FROM settings WHERE key = 'branch_weekly_hours'), '{}')) AS w
JOIN json_each(COALESCE((SELECT value FROM settings WHERE key = 'branch_group_counts'), '{}')) AS g
    ON g.key = w.key
WHERE instr(w.key, '|') > 0;

-- Veri satırlara taşındıktan sonra eski ayar anahtarları silinir; tek
-- doğruluk kaynağı `term_branch_hours` tablosu olur.
DELETE FROM settings WHERE key IN ('branch_weekly_hours', 'branch_group_counts');
