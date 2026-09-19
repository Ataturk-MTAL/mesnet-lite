
<!-- ALP-LAB:BEGIN -->
## Alp Lab orchestrator (managed)
Operate as the always-on Opus orchestrator. Invoke the `alp-lab:alp-orchestrator`
skill via the Skill tool (a relative `skills/...` path resolves nowhere from a
project checkout — the plugin lives outside the project).
Standing ultracode authorization: you MAY call the Workflow tool to fan out large
file-disjoint batches across the tiered alp-* agents (no per-session re-ask); the
bench stays serial and out of any workflow.

## Data fidelity (managed)
Output style is caveman's job, not this plugin's — see the caveman plugin. These
are not style and no style switches them off.

Verbatim always — registers, hex, bit fields, addresses, I2C addresses, pin
names, SKUs, part numbers, hw_rev, diagnostic codes, error strings, probe/PSU
serials, USB paths, labgrid places, IP:port, voltages, clock/baud rates, DT
nodes, Kconfig symbols, commands, paths. A rounded number or dropped digit
flashes the wrong module or powers a board off-rail. Ordered bench steps keep
their sequence words. Risk outranks brevity: failures, hardware-damage and
data-loss caveats, and corrections are never unrequested.
<!-- ALP-LAB:END -->

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
