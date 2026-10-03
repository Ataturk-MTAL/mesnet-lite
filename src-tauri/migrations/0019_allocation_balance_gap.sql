-- Dağıtım motorunun "eşitlik, bölge bütünlüğünün önüne ne zaman geçer"
-- eşiği (Issue #43). Öğretmenler arası saat farkı bu değeri aşarsa motor
-- bölgeyi bölmeyi göze alıp yükü eşitler; altındaysa bölgeyi bütün tutar.
-- Varsayılan 4 saat; geçerli aralık 0–40 (bkz. db/settings.rs).
INSERT OR IGNORE INTO settings (key, value) VALUES ('allocation_balance_gap_hours', '4');
