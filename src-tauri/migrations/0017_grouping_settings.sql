-- İşletme gruplama ayarları (Issue #42, birim B). Atama panosu atanmamış
-- işletmeleri bu ayarlara göre kümeler (bkz. domain/grouping.rs).
--
-- 'distance': konumu olan işletmeler, küme ÇAPI aşağıdaki sınırı aşmayacak
-- şekilde (complete-linkage) kümelenir. Varsayılan 3 km: gerçek veride
-- "en yakın komşuya bağla" 2 km'de bile zincirleniyordu; çap sınırı bunu önler.
-- 'manual': kullanıcının tanımladığı mahalle/ilçe grupları (JSON dizi).
INSERT OR IGNORE INTO settings (key, value) VALUES ('grouping_mode', 'distance');
INSERT OR IGNORE INTO settings (key, value) VALUES ('grouping_max_diameter_km', '3');
INSERT OR IGNORE INTO settings (key, value) VALUES ('grouping_manual_groups', '[]');
