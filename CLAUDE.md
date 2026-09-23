## MESNET.Lite — iş bölümü

Bu projede teşhis ile kodlama ayrı. Kullanıcının kuralı: **sorunu sen teşhis
et, kodlamayı Sonnet ajanlar yapsın.**

**Senin işin:** sorunu bulmak, kök nedeni kanıtlamak, kararı vermek, brief
yazmak, gelen işi doğrulamak, birleştirmek, commit etmek.

**Ajanların işi:** kod yazmak.

Uygulama kodunu kendin yazma. `.claude/agents/` altındaki iki ajana dağıt:

| Ajan | Kapsam |
|---|---|
| `mesnet-rust` | `src-tauri/` — domain, db, commands, services, migrations |
| `mesnet-vue`  | `src/` — görünümler, bileşenler, API istemcisi, `labels.ts` |

Bu tanımlar proje kurallarını (Türkçe yorum, `labels.ts` zorunluluğu, TDD,
kapılar, rapor biçimi) zaten taşıyor. Brief'te tekrarlama; yalnızca **ne
bozuk, hangi dosyalar, sözleşme ne** yaz.

İstisna — bunları kendin yapabilirsin: yapılandırma dosyaları, `CLAUDE.md`,
ajan tanımları, tek satırlık birleştirme düzeltmeleri, ve teşhis için gereken
geçici deneme kodu (commit etme).

**Eşzamanlılık.** Birden çok ajan aynı anda koşacaksa dosyaları ayrık olmalı.
Her brief'te açık bir "dokunma" listesi ver. Aynı dosyaya iki ajan gönderme;
sıraya koy.

**Bağımlılık.** Arka uç yeni bir alan üretiyorsa, arayüz ajanına o alanların
camelCase adlarını birebir ver. Tahmin ederse uydurur.

**Doğrulama.** Ajan raporunu olduğu gibi kabul etme. Test sayılarını kendin
koş, değiştirilen mevcut testlere bak, ölü kod bırakıp bırakmadığını kontrol
et. Bu projede daha önce hiç çağrılmayan mevzuat kontrolleri yüzünden kurallar
fiilen uygulanmadı.

**Not:** Yukarıdaki alp-lab yönlendirme tablosu bu projeye uymuyor —
`alp-implementor` ve `alp-investigator` alp-sdk / signex / tan-cli
skill'lerini yükler, MESNET'i tanımaz. Bu projede onların yerine yukarıdaki
iki ajanı kullan.

## Dal kuralı (zorunlu geliştirme kısıtı)

Ayrıntı: `CONTRIBUTING.md`.

- `main` ve `dev`'e **doğrudan commit ya da push YOK**. İkisine de yalnız PR ile girilir.
- Her özellik/hata düzeltmesi `dev`'den ayrılan bir dalda yapılır:
  `git switch dev && git pull && git switch -c <tür>/<kısa-ad>` (`feat/`, `fix/`, `docs/`, `chore/`, `refactor/`, `test/`).
- İşe başlamadan önce bulunulan dalı kontrol et; `main` ya da `dev` üzerindeysen önce dal aç.
- İş dalı → `dev` PR'ı; CI yeşil olmadan birleştirme. Sürüm: `dev` → `main` PR'ı, sonra `main`'de `vX.Y.Z` etiketi.
- Ajan brief'lerinde de çalışılacak dalı belirt; ajan `main`/`dev` üzerinde commit atmaz.

## Kişisel veri (zorunlu)

Depo herkese açık. Gerçek öğrenci/veli/öğretmen/işletme adı, numarası, telefonu,
e-postası, gerçek CSV/Excel dışa aktarımı ya da `.db` dosyası hiçbir commit'e
girmez. Testlerde yalnız kurgusal veri. Gerçek veritabanından okunan değerleri
test/fixture'a kopyalama.
