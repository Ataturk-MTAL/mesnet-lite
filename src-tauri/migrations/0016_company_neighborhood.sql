-- Adres çözümü (Nominatim) uzun sokak/kapı numaralı adreslerde sonuç
-- vermiyor; `<mahalle>, <ilçe>, <il>, Türkiye` sorgusu ise mahallelerin büyük
-- çoğunluğunu `suburb` düzeyinde çözüyor (Issue #42). Bu yüzden mahalle,
-- `address_text`'ten AYRI bir sütunda tutulur. Mevcut satırların mahalle
-- geri doldurması SQL ile yapılamaz (ayrıştırma Rust'ta; bkz.
-- domain/address.rs::parse_neighborhood, db/mod.rs::backfill_company_neighborhoods).
ALTER TABLE companies ADD COLUMN neighborhood TEXT NOT NULL DEFAULT '';

-- Konumun ne kadar kesin olduğu: 'address' tam adresten, 'neighborhood'
-- mahalle merkezinden (yaklaşık), 'manual' haritadan elle işaretlenmiş.
-- Boş = konum yok. Yaklaşık konum arayüzde ayırt edilebilsin diye
-- `geocode_status`tan ayrı tutulur ('resolved' ikisini de kapsar).
ALTER TABLE companies ADD COLUMN geocode_precision TEXT NOT NULL DEFAULT ''
    CHECK (geocode_precision IN ('', 'address', 'neighborhood', 'manual'));

-- Var olan satırlar: bu göçten önce yalnız tam adres çözümü vardı.
UPDATE companies SET geocode_precision = CASE
    WHEN geocode_status = 'manual' THEN 'manual'
    WHEN geocode_status = 'resolved' AND latitude IS NOT NULL AND longitude IS NOT NULL THEN 'address'
    ELSE ''
END;
