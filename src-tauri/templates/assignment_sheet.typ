// Koordinatör Görevlendirme Çizelgesi (MADDE 15/2)
//
// Veri, Rust tarafından `sys.inputs.data` üzerinden tek bir JSON dizgisi olarak
// gelir; şablon burada onu çözer ve tabloları üretir. Şablon veriye bağımlıdır,
// Typst kaynağı asla dizgi birleştirmesiyle üretilmez.
#import sys: inputs
#let data = json(bytes(inputs.data))

#set page(
  paper: "a4",
  flipped: true,
  margin: (x: 1.4cm, y: 1.4cm),
)
#set text(font: "DejaVu Sans", lang: "tr", size: 9pt)
#set table(stroke: 0.5pt + gray)
#set par(justify: false)

// Sayfa başlığı: okul adı, belge adı, eğitim-öğretim yılı, yazdırılma tarihi.
#align(center)[
  #text(size: 14pt, weight: "bold")[#data.schoolName]
  #v(0.25em)
  #text(size: 12.5pt, weight: "bold")[Koordinatör Görevlendirme Çizelgesi]
  #v(0.2em)
  #text(size: 9.5pt)[
    Eğitim-Öğretim Yılı: #data.academicYear #h(1.5em) Yazdırılma Tarihi: #data.printedAt
  ]
]

#v(0.5em)

// Zorlanmış (kural dışı) atama varsa uyarı bandı gösterilir.
#if data.hasForcedRows [
  #block(
    width: 100%,
    fill: rgb("#fff3cd"),
    stroke: 1pt + rgb("#997404"),
    inset: 7pt,
    radius: 2pt,
  )[
    #text(weight: "bold", fill: rgb("#664d03"))[DİKKAT:] Bu çizelgede kural dışı (zorlanmış)
    atamalar bulunmaktadır; ilgili satırlar işaretlenmiştir.
  ]
  #v(0.5em)
]

// Öğretmen başına tablo: her tablonun kendi başlığı ve "Ara Toplam" satırı var.
#let hours-cell(row) = if row.isHonorary [Fahri] else [#row.hours]

#let forced-mark(row) = if row.isForced [
  #text(fill: rgb("#b42318"), weight: "bold")[ (Zorlanmış)]
] else []

#for teacher in data.teachers [
  #text(size: 10.5pt, weight: "bold")[#teacher.name]
  #v(0.15em)
  #table(
    columns: (2.1fr, 3.1fr, 0.9fr, 2.6fr, 1.5fr, 0.9fr),
    align: (left, left, center, left, center, center),
    table.header(
      [*İşletme*],
      [*Adres*],
      [*Öğrenci Sayısı*],
      [*Öğrenciler*],
      [*Ziyaret Gün/Saat*],
      [*Haftalık Saat*],
    ),
    ..teacher.rows.map(row => (
      [#row.companyName#forced-mark(row)],
      [#row.address],
      [#row.studentCount],
      [#row.students],
      [#row.visitSchedule],
      hours-cell(row),
    )).flatten(),
    table.cell(colspan: 5, align: right, fill: luma(235))[*Ara Toplam*],
    table.cell(fill: luma(235))[*#teacher.subtotalHours*],
  )
  #v(0.6em)
]

// Belgenin sonunda dönem geneli toplam saat.
#align(right)[
  #text(weight: "bold", size: 10.5pt)[GENEL TOPLAM: #data.grandTotalHours saat]
]

#v(1.8em)

// Onay imza blokları: koordinatör, okul müdürü, il/ilçe millî eğitim müdürlüğü.
#let signature-block(title) = align(center)[
  #v(2em)
  #line(length: 100%, stroke: 0.5pt)
  #v(0.3em)
  #text(size: 9pt)[#title]
]

#grid(
  columns: (1fr, 1fr, 1fr),
  gutter: 1.2cm,
  signature-block("Koordinatör Müdür Yardımcısı"),
  signature-block("Okul Müdürü"),
  signature-block("İl/İlçe Millî Eğitim Müdürlüğü Onayı"),
)
