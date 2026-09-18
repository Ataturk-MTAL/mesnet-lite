// Öğretmen Ziyaret Listeleri: her öğretmen için ayrı bir sayfa.
//
// Veri, Rust tarafından `sys.inputs.data` üzerinden tek bir JSON dizgisi olarak
// gelir; şablon burada onu çözer ve sayfaları üretir.
#import sys: inputs
#let data = json(bytes(inputs.data))

#set page(
  paper: "a4",
  margin: (x: 1.6cm, y: 1.6cm),
)
#set text(font: "DejaVu Sans", lang: "tr", size: 10pt)
#set table(stroke: 0.5pt + gray)
#set par(justify: false)

#let hours-cell(row) = if row.isHonorary [Fahri] else [#row.hours]

#for (index, teacher) in data.teachers.enumerate() [
  #align(center)[
    #text(size: 13pt, weight: "bold")[#teacher.name]
    #v(0.2em)
    #text(size: 11pt)[#data.schoolName]
    #v(0.1em)
    #text(size: 9.5pt)[Eğitim-Öğretim Yılı: #data.academicYear]
  ]

  #v(0.7em)

  #table(
    columns: (1.9fr, 2.5fr, 1.1fr, 1.4fr, 2fr, 1.6fr, 0.9fr),
    align: (left, left, left, left, left, center, center),
    table.header(
      [*İşletme*],
      [*Adres*],
      [*Telefon*],
      [*Yetkili*],
      [*Öğrenciler*],
      [*Ziyaret Gün/Saat*],
      [*Haftalık Saat*],
    ),
    ..teacher.rows.map(row => (
      [#row.companyName],
      [#row.address],
      [#row.phone],
      [#row.contact],
      [#row.students],
      [#row.visitSchedule],
      hours-cell(row),
    )).flatten(),
  )

  #v(0.7em)

  #align(right)[
    #text(weight: "bold")[Toplam Saat: #teacher.totalHours]
  ]

  #if index < data.teachers.len() - 1 [
    #pagebreak()
  ]
]
