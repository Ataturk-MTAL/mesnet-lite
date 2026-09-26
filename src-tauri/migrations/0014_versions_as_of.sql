-- Kayıtlı bir sürümün hangi tarihe göre üretildiği (bkz. `db::read_at::ReadAt`).
-- NULL: sürüm `Latest` (güncel/bugünkü duruma göre) üretildi. Dolu:
-- 'YYYY-MM-DD', beş projeksiyonun o gündeki hâliyle üretildi.
--
-- Tarih, veri kopyasıyla (`file_name`) BİRLİKTE saklanır: tarihçe sonradan
-- düzeltilirse (ör. bir olay iptal edilip yeniden yazılırsa) aynı `as_of`
-- günün görünümü değişebilir; sürümün dayanağının birebir yeniden
-- üretilebilmesi için hem o anki veri hem de o anki "hangi güne bakıldığı"
-- donmuş kalmalıdır.
ALTER TABLE versions ADD COLUMN as_of TEXT;
