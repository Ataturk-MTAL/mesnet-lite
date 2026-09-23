#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""e-Okul sınıf listesi (.XLS) test fixture üretici.

`student_list_import.rs`/`student_list_apply.rs` testleri gerçek e-Okul
dışa aktarımlarına ihtiyaç duyar, ama gerçek dosyalar öğrenci kişisel
verisi içerdiği için depoya commit EDİLEMEZ. Bu betik, gerçek dosyaların
YAPISINI (BIFF8/.XLS, sayfa düzeni, başlık satırı konumu, sütun adları ve
konumları) birebir taklit eden ama TAMAMEN KURGUSAL öğrenci verisiyle dolu
dosyalar üretir; böylece testler CI dahil her ortamda gerçek veri olmadan
koşabilir.

Bu betik GERÇEK DOSYA OKUMAZ — yalnızca aşağıdaki sabit kurgusal listeden
üretir. Ad/soyad çiftleri bilinçli olarak yabancı/anlamsız seçildi (birkaçı
Türkçe harf içerir ama tanınabilir bir Türk ismi DEĞİLDİR) ki gerçek bir
öğrenciyle karışmasın.

Çalıştırmak için: `python3 -m venv .venv && .venv/bin/pip install xlwt`
sonra `.venv/bin/python src-tauri/scripts/make_eokul_fixtures.py`.
"""

import os
import xlwt

# Kurgusal öğrenciler: (öğrenci no, ad, soyad, dal). Numaralar gerçek
# öğrenci no aralığıyla (dört haneli, 1000-8999) çakışmasın diye 9xxx
# bandında seçildi.
CLASS_C_BRANCH = "Elektronik ve Haberleşme"
CLASS_D_BRANCH_A = "Elektrik Tesisatları ve Dağıtımı"
CLASS_D_BRANCH_B = "Endüstriyel Bakım Onarım"

CLASS_C_STUDENTS = [
    (9001, "ALVAR", "QUENNET"),
    (9002, "MIRELLE", "OSKARSEN"),
    (9003, "TOBIN", "VASQUEZ"),
    (9004, "ZÜLFA", "KVARNSTRÖM"),
    (9005, "BRANWEN", "ASHCROFT"),
    (9006, "DARIUSZ", "WOLCZANSKI"),
    (9007, "ODALYS", "FIGUEROA"),
    (9008, "LENNART", "VIKSTROM"),
    (9009, "ROSALIND", "MCALLISTER"),
    (9010, "YAGO", "ESPINOZA"),
    (9011, "FENNEC", "OYELARAN"),
    (9012, "INGRID", "SOLBAKKEN"),
    (9013, "CASPIAN", "DELACROIX"),
    (9014, "NADEZHDA", "PETROVSKA"),
    (9015, "TAMSIN", "LOCKHART"),
    (9016, "ORION", "MBEKI"),
    (9017, "SIGRUN", "THORVALDSEN"),
]

# İlk 8'i A dalında (Elektrik Tesisatları ve Dağıtımı), kalan 9'u B dalında
# (Endüstriyel Bakım Onarım) — testin sınadığı 8/9 bölünmesi bilerek korundu.
CLASS_D_STUDENTS = [
    (9101, "WREN", "CALLOWAY", CLASS_D_BRANCH_A),
    (9102, "GÜNVOR", "THALREN", CLASS_D_BRANCH_A),
    (9103, "IMOGEN", "STRAND", CLASS_D_BRANCH_A),
    (9104, "KASPAR", "LINDQVIST", CLASS_D_BRANCH_A),
    (9105, "İNGEBORG", "MACLENNAN", CLASS_D_BRANCH_A),
    (9106, "HOLGER", "BRANDSEN", CLASS_D_BRANCH_A),
    (9107, "SAOIRSE", "DUNMORE", CLASS_D_BRANCH_A),
    (9108, "THADDEUS", "OKONKWO", CLASS_D_BRANCH_A),
    (9109, "LUCIEN", "FAIRWEATHER", CLASS_D_BRANCH_B),
    (9110, "ASTRID", "NDIAYE", CLASS_D_BRANCH_B),
    (9111, "MAGNUS", "OYELOWO", CLASS_D_BRANCH_B),
    (9112, "PERPETUA", "VANCE", CLASS_D_BRANCH_B),
    (9113, "OLWEN", "ÇAKRABARTI", CLASS_D_BRANCH_B),
    (9114, "FIACHRA", "STEINGRUBER", CLASS_D_BRANCH_B),
    (9115, "VESNA", "OKAFOR", CLASS_D_BRANCH_B),
    (9116, "ROALD", "BENAVIDES", CLASS_D_BRANCH_B),
    (9117, "HELGA", "ARMITAGE", CLASS_D_BRANCH_B),
]

# Gerçek e-Okul dışa aktarımının başlık hücresi (A1) budur; okulun kendi
# adı/adresi kişisel veri değil, projenin geri kalanında zaten açık
# kullanılıyor (bkz. proje adı).
TITLE_TEMPLATE = (
    "T.C.\nMERSİN VALİLİĞİ\nToroslar / Atatürk Mesleki Ve Teknik Anadolu Lisesi "
    "Müdürlüğü\nAMP - 12. Sınıf / {section} Şubesi (ELEKTRİK-ELEKTRONİK "
    "TEKNOLOJİSİ ALANI) Sınıf Listesi"
)

HEADER_ROW = 3
FIRST_DATA_ROW = 4


def _write_r076(sheet, students):
    """Dalı sütunlu rapor: S.No=0, Öğrenci No=1, Adı=3, Soyadı=6, Dalı=10."""
    sheet.write(HEADER_ROW, 0, "S.No")
    sheet.write(HEADER_ROW, 1, "Öğrenci No")
    sheet.write(HEADER_ROW, 3, "Adı")
    sheet.write(HEADER_ROW, 6, "Soyadı")
    sheet.write(HEADER_ROW, 10, "Dalı")

    for i, (no, first, last, branch) in enumerate(students):
        row = FIRST_DATA_ROW + i
        sheet.write(row, 0, i + 1)
        sheet.write(row, 1, no)
        sheet.write(row, 3, first)
        sheet.write(row, 6, last)
        sheet.write(row, 10, branch)


def _write_r020(sheet, students):
    """Dalı sütunu OLMAYAN rapor: Soyadı 7'de (R076'dan farklı ofset),
    onun yerine Cinsiyeti/Pansiyon Durum sütunları var (kurgusal, test
    tarafından okunmuyor ama gerçek dosya düzenini taklit ediyor)."""
    sheet.write(HEADER_ROW, 0, "S.No")
    sheet.write(HEADER_ROW, 1, "Öğrenci No")
    sheet.write(HEADER_ROW, 3, "Adı")
    sheet.write(HEADER_ROW, 7, "Soyadı")
    sheet.write(HEADER_ROW, 11, "Cinsiyeti")
    sheet.write(HEADER_ROW, 13, "Pansiyon Durum")

    for i, (no, first, last, _branch) in enumerate(students):
        row = FIRST_DATA_ROW + i
        sheet.write(row, 0, i + 1)
        sheet.write(row, 1, no)
        sheet.write(row, 3, first)
        sheet.write(row, 7, last)
        sheet.write(row, 11, "K" if i % 2 == 0 else "E")
        sheet.write(row, 13, "Gündüzlü")


def make_file(path, section, writer_fn, students):
    book = xlwt.Workbook(encoding="utf-8")
    sheet = book.add_sheet("Sınıf Listesi")
    sheet.write(0, 0, TITLE_TEMPLATE.format(section=section))
    writer_fn(sheet, students)

    # Gerçek dosyalardaki gibi son satırdan sonra sayısal olmayan bir özet
    # satırı: ayrıştırıcının "S.No sayı olmaktan çıkınca dur" kuralını
    # (bkz. `read_student_rows`) fixture'da da sınamış oluyoruz.
    summary_row = FIRST_DATA_ROW + len(students)
    sheet.write(summary_row, 0, "Toplam Öğrenci Sayısı")

    book.save(path)


def main():
    out_dir = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "src",
        "services",
        "fixtures",
        "eokul",
    )
    os.makedirs(out_dir, exist_ok=True)

    class_c_r076 = [(no, first, last, CLASS_C_BRANCH) for no, first, last in CLASS_C_STUDENTS]

    make_file(os.path.join(out_dir, "r076_12c.xls"), "C", _write_r076, class_c_r076)
    make_file(os.path.join(out_dir, "r076_12d.xls"), "D", _write_r076, CLASS_D_STUDENTS)
    make_file(os.path.join(out_dir, "r020_12c.xls"), "C", _write_r020, class_c_r076)
    make_file(os.path.join(out_dir, "r020_12d.xls"), "D", _write_r020, CLASS_D_STUDENTS)

    print(f"4 kurgusal e-Okul fixture'ı yazıldı: {out_dir}")


if __name__ == "__main__":
    main()
