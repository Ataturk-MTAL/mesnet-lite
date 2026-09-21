// İşletme Belirleme Komisyon Tutanağı
//
// Okulun kendi Excel şablonunun aynı düzeni: A4 dikey; başlık kutusu, müdürlüğe
// hitap, açıklama paragrafı, imza şeridi, öğrenci başına satırlı tablo, onay ve
// açıklama blokları. Veri Rust'tan `sys.inputs.data` üzerinden tek JSON dizgisi
// olarak gelir (`MinutesData`); metinlerin hiçbiri burada yeniden kurulmaz, bu
// yüzden Excel çıktısıyla ayrışamaz. Typst kaynağı dizgi birleştirmesiyle
// üretilmez.
#import sys: inputs
#let data = json(bytes(inputs.data))

#set page(paper: "a4", margin: (x: 0.7cm, y: 1.6cm))
#set text(font: "DejaVu Sans", lang: "tr", size: 8pt)
#set par(justify: false, leading: 0.55em)
#set block(spacing: 0pt)

// Veri içindeki "\n" karakterleri gerçek satır sonuna çevrilir.
#let lines(value) = value.split("\n").join(linebreak())

// Şablonun sütun oranları (Excel sütun genişlikleri): sıra no, işletme,
// öğrenci, uzaklık, koordinatör, görev günü, ücret.
#let column-widths = (6.57fr, 39.43fr, 32.43fr, 14fr, 35.71fr, 16.86fr, 10.14fr)

// A1:G4 — üç satırlık çerçeveli başlık.
#block(width: 100%, stroke: 0.5pt, inset: (x: 6pt, y: 9pt))[
  #align(center)[
    #text(size: 10.5pt, weight: "bold")[
      #data.yearLine \
      #data.schoolLine \
      #data.fieldLine
    ]
  ]
]

#v(1.1em)
// A6:G6 — müdürlüğe hitap.
#align(center)[#text(size: 9pt, weight: "bold")[#data.addresseeLine]]
#v(1.1em)

// A8:G11 — açıklama paragrafı; şablondaki 16 boşluk yerine ilk satır girintisi.
#par(first-line-indent: (amount: 2.2em, all: true))[#data.intro]

#v(1.1em)
#pad(left: 6.57 / 155 * 100% + 0.3em)[#data.closingLine]
#v(1.1em)

// A15:G17 — imza etiketleri: Alan Şefi (A:B) ve Alan Öğretmenleri (C:G).
// Öğretmenler ızgarası eskiden yalnız C:E genişliğindeydi; kullanıcının
// "biraz boşlukla 4 sütunlu bir yapı" isteği için önceden boş duran F:G de
// bu genişliğe katıldı (bkz. Excel karşılığı: aynı gerekçe).
#let teachers-grid-width = 82.14fr + 26.99fr
#grid(
  columns: (46fr, teachers-grid-width),
  align: center + horizon,
  lines(data.chiefLabel), lines(data.teachersLabel),
)
#v(0.8em)

// Alan şefi ızgaraya karışmaz (kullanıcı isteği: "ayrı kalır"); okulda en
// fazla bir bölüm şefi olduğundan (bkz. `decide/chief.rs`) tek satır yeterli,
// kendi dar sütununda (A:B) ortalanır.
//
// Alan öğretmenleri kullanıcının tarif ettiği biçimde basılır: "4 sütunlu,
// soldan sağa, satır satır aşağı akan bir ızgara; ismin altında unvan olsun".
// `grid` hücreleri zaten sırayla soldan sağa, satır dolunca alta yerleştirir
// (sütun sütun DEĞİL) — bu yüzden `field-teacher-entries` listesini sırayla
// vermek yeterli, satır/sütun indeksini elle hesaplamaya gerek yok. Her
// hücrenin altındaki `#v` boşluğu, kişinin adının altına gerçekten imza
// atabileceği yeri ayırır (bu bir imza şeridi, yalnız bir isim listesi değil).
//
// Kullanıcı isteği ("imza için çok az daha aralık"): hem hücre altı boşluk
// hem satırlar arası boşluk bir miktar artırıldı (1.6em→1.9em, 0.6em→0.8em);
// tek sayfaya sığma 12 kişilik referans senaryoda (`a_full_signature_roster_
// of_twelve_teachers_compiles`) elle doğrulanmıştır.
#let signature-space-below-name = 1.9em
#let signature-row-gutter = 0.8em
#let field-teacher-cell(t) = align(center + top)[
  #text(weight: "bold")[#t.name] \
  #text(size: 6.5pt, fill: gray.darken(20%))[#t.title]
  #v(signature-space-below-name)
]
#let field-teacher-entries = data.fieldTeachers.map(field-teacher-cell)
#let field-teacher-grid = if field-teacher-entries.len() > 0 {
  grid(
    columns: (1fr, 1fr, 1fr, 1fr),
    column-gutter: 1.4em,
    row-gutter: signature-row-gutter,
    ..field-teacher-entries,
  )
} else { [] }
// Alan şefinin adı, alan öğretmenlerindeki isimlerle (field-teacher-cell)
// aynı ağırlıkta KALIN basılır (kullanıcı isteği: imza şeridinde tutarlı
// görünsün); "Alan Şefi/İmza" başlığı (chiefLabel) düz kalmaya devam eder.
#grid(
  columns: (46fr, teachers-grid-width),
  align: (center + top, left + top),
  [#align(center)[#text(weight: "bold")[#data.chiefName]]], field-teacher-grid,
)
#v(1.1em)

// Bir satırın, YALNIZ o satırda başlayan hücreleri. Typst hücreleri sütun
// sırasıyla, önceki satırlardan uzanan `rowspan` hücrelerinin altını atlayarak
// yerleştirir; bu yüzden birleşik bir hücre sadece ilk satırında verilir.
//  - D (uzaklık), F (görev günü), G (ücret): işletme başına dikey birleşik.
//    `breakable: false` bir işletmenin sayfalar arasında bölünmesini önler.
//  - E (koordinatör): aynı öğretmenin ardışık işletmeleri boyunca birleşik
//    (referans şablondaki gibi); uzun olabileceğinden sayfa arasında bölünebilir.
//    Atanmamış işletmenin E hücresi boş kalır ve birleşmez.
#let row-cells(i) = {
  let row = data.rows.at(i)
  let company = data.groups.find(g => g.start == i)
  let teacher = data.teacherGroups.find(g => g.start == i)

  let cells = ([#row.index], row.companyName, row.studentName)
  if company != none {
    cells.push(table.cell(rowspan: company.len, breakable: false)[#row.distanceLabel])
  }
  if teacher != none {
    cells.push(table.cell(rowspan: teacher.len)[#row.teacher])
  } else if row.teacher == "" {
    cells.push([])
  }
  if company != none {
    cells.push(table.cell(rowspan: company.len, breakable: false)[#row.day])
    cells.push(table.cell(rowspan: company.len, breakable: false)[#row.hoursLabel])
  }
  cells
}

#table(
  columns: column-widths,
  align: (center + horizon, left + horizon, left + horizon, center + horizon, center + horizon, center + horizon, center + horizon),
  stroke: 0.5pt,
  inset: (x: 4pt, y: 5pt),
  // Tablo sayfayı aşarsa başlık satırı her sayfada yinelenir (varsayılan).
  table.header(..data.columnHeaders.map(header => lines(header.replace("(", " (")))),
  ..range(data.rows.len()).map(row-cells).flatten(),
)

// Onay bloğu dört ayrı alana bölünmüştür (approvalLine, approvalDateLine,
// principalName, principalTitleLine) ki yalnız müdür adı kalın basılabilsin;
// müdür adı boşken ayarlardan gelen noktalı yer tutucu bu alanda basılır ve
// aynı şekilde kalın görünür (davranış Rust tarafında değişmedi, bkz.
// `commission_minutes::or_placeholder`). Onay ve açıklama blokları tablonun
// hemen ardından gelir ve bölünmez.
#block(width: 100%, stroke: 0.5pt, inset: 8pt, breakable: false)[
  #align(center + horizon)[#block(height: 2.6cm)[#align(horizon)[
    #data.approvalLine \
    #data.approvalDateLine \
    #text(weight: "bold")[#data.principalName] \
    #data.principalTitleLine
  ]]]
]
#block(width: 100%, stroke: 0.5pt, inset: 6pt, breakable: false)[
  #align(center)[#data.noteText]
]
