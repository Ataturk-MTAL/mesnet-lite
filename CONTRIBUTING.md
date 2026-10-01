# Katkı ve geliştirme kuralları

## Dallar

| Dal | Amaç | Kim yazar |
|---|---|---|
| `main` | Yayınlanan sürümler. Her `v*` etiketi buradan çıkar. | Yalnız `dev` → `main` PR'ı |
| `dev` | Bir sonraki sürümün birikim dalı. | Yalnız iş dallarından gelen PR'lar |
| `feat/…`, `fix/…`, `docs/…`, `chore/…`, `refactor/…`, `test/…` | Tek bir iş. | Geliştirici |

**Kural (zorunlu):**

1. Her özellik ve her hata düzeltmesi, **`dev`'den ayrılan** kendi dalında yapılır:
   ```bash
   git switch dev && git pull
   git switch -c fix/kilitli-satir-geri-al
   ```
2. `main`'e ve `dev`'e **doğrudan commit/push yapılmaz**. Her değişiklik bir Pull Request ile girer.
3. İş dalı PR ile **`dev`'e** birleştirilir; CI yeşil olmadan birleştirilmez.
4. Sürüm zamanı `dev` → `main` PR'ı açılır; birleşince sürüm kendiliğinden derlenip yayımlanır ve `vX.Y.Z` etiketi o anda oluşur (bkz. *Sürüm çıkarma*).
5. Acil düzeltme (hotfix) de aynı yoldan gider: `dev`'den dal → `dev` → `main`.

## Dal adları

`<tür>/<kısa-açıklama>` — tür, commit türüyle aynıdır: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `perf`, `ci`.

## Commit mesajları

```
<tür>: <ne değişti — kullanıcının gözünden>

<neden; gerekiyorsa ayrıntı>
```

## Birleştirmeden önce

```bash
pnpm exec vue-tsc --noEmit -p tsconfig.json
pnpm exec vitest run
cd src-tauri && cargo test
```

## Kişisel veri

Depo herkese açıktır. **Gerçek öğrenci, veli, öğretmen ya da işletme verisi** (ad, numara, telefon, e-posta, gerçek CSV/Excel dışa aktarımı, veritabanı dosyası) hiçbir commit'e girmez — testlerde yalnız kurgusal veriler kullanılır. `data/`, `*.csv`, `*.xlsx` ve `*.db` dosyaları `.gitignore` ile dışarıda tutulur.

## Sürüm çıkarma

1. Bir `chore/` dalında `package.json`, `src-tauri/Cargo.toml` ve `src-tauri/tauri.conf.json` sürümünü artır; `release-notes/X.Y.Z.md` dosyasına kullanıcıya dönük Türkçe sürüm notlarını yaz. Bu metin hem GitHub sürüm sayfasında hem uygulama içi güncelleme penceresinde görünür. Dosya yoksa sürüm işi durur.
2. Bu dalı `dev`'e, ardından `dev` → `main` PR'ını birleştir (CI yeşil olmadan değil).
3. `main`'e gelen push `Sürüm` iş akışını başlatır: taslak sürüm açılır, Windows, macOS, Linux ve MSIX derlenip eklenir, hepsi bitince taslak yayımlanır ve `vX.Y.Z` etiketi oluşur. Elle etiket gönderilmez.
4. Bir derleme düşerse sürüm taslakta kalır; Actions'ta işi yeniden çalıştırmak aynı taslağa devam eder.
