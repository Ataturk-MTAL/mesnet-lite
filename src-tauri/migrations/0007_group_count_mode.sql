-- Grup sayısı artık öğrenci sayısından otomatik hesaplanır (Norm Kadro
-- Yönetmeliği MADDE 22/1-ç tablosu; bkz. domain/group_count.rs). Kullanıcı
-- isterse bir satırın grup sayısını elle girer; bu sütun satırın hangi modda
-- olduğunu tutar.
--
--   0 = otomatik: etkin grup sayısı OKUMA ANINDA öğrenci sayısından
--       hesaplanır, böylece öğrenci sayısı değişince havuz kendiliğinden
--       güncellenir. `group_count` yalnız önbellektir ve kaynak doğruluk
--       değildir.
--   1 = elle: etkin grup sayısı `group_count` sütunudur ve korunur.
--
-- Mevcut satırlar otomatik moda geçer (DEFAULT 0). Bu bilinçli bir karardır:
-- eskiden elle girilmiş değerler (ör. 12/C Elektronik Haberleşme, 16 öğrenci,
-- elle 2) tablonun verdiği değere (1) iner. `group_count` değerleri
-- değişmeden kalır; kullanıcı isterse satırı yeniden elle işaretler.
ALTER TABLE term_branch_hours
    ADD COLUMN is_group_manual INTEGER NOT NULL DEFAULT 0
    CHECK (is_group_manual IN (0, 1));
