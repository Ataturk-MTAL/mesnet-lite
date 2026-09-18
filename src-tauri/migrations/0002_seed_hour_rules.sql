-- İşletme saat tavanı başlangıç tablosu.
-- SATIRLAR GİDİŞ-DÖNÜŞ km'dir (tek yönün iki katı).
--
--  gidiş-dönüş km | 1-2 öğr. | 3-4 öğr. | 5-6 öğr. | 6+ öğr.
--  0 - 1          |    2     |    3     |    4     |   5
--  1 - 3          |    4     |    5     |    6     |   7
--  3 - 5          |    6     |    7     |    8     |   9
--  5 +            |    8     |    9     |   10     |  11
--
-- Not: 5-6 ve 6+ sütunları 6 öğrencide çakışır. "En dar kural" sıralaması
-- (spec §6) 6 öğrenciyi 5-6 sütununa yerleştirir. Belirlenimcidir.

INSERT INTO company_hour_rules
    (min_distance_km, max_distance_km, min_students, max_students, max_hours)
VALUES
    (0.0, 1.0, 1, 2,    2),
    (0.0, 1.0, 3, 4,    3),
    (0.0, 1.0, 5, 6,    4),
    (0.0, 1.0, 6, NULL, 5),

    (1.0, 3.0, 1, 2,    4),
    (1.0, 3.0, 3, 4,    5),
    (1.0, 3.0, 5, 6,    6),
    (1.0, 3.0, 6, NULL, 7),

    (3.0, 5.0, 1, 2,    6),
    (3.0, 5.0, 3, 4,    7),
    (3.0, 5.0, 5, 6,    8),
    (3.0, 5.0, 6, NULL, 9),

    (5.0, NULL, 1, 2,    8),
    (5.0, NULL, 3, 4,    9),
    (5.0, NULL, 5, 6,   10),
    (5.0, NULL, 6, NULL, 11);
