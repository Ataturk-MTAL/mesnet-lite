-- Atamanın kaynağı (Issue #43): `manual` = kullanıcı elle yaptı (zorlamalı ya
-- da değil), `proposal` = uygulanmış bir dağıtım önerisinden geldi. "Baştan
-- dağıt" kipinde motor yalnız `proposal` kaynaklı atamaları yeniden
-- düzenleyebilir; `manual` olanlara dokunmaz.
--
-- Mevcut satırlar kullanıcının elle yaptığı atamalardır, bu yüzden
-- varsayılan `manual`. Sütun yalnız önbellektir: gerçek kaynak
-- `coordinator_assigned` olayının `state.source` alanıdır ve projeksiyon
-- yeniden kurulunca (`revoke`/`correct` dahil) olaydan geri yüklenir.
ALTER TABLE coordination_periods
    ADD COLUMN source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'proposal'));
