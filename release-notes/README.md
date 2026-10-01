# Sürüm notları

Her sürüm için bu klasörde `X.Y.Z.md` (örn. `0.1.6.md`) bulunur. `Sürüm`
iş akışı (`.github/workflows/release.yml`) bu dosyayı okur:

- GitHub sürüm sayfasının başına konur (altına kurulum uyarısı eklenir:
  `.github/release-install-note.md`).
- Uygulama içi güncelleme penceresinde kullanıcıya gösterilir (`latest.json`
  içindeki `notes`).

Dosya yoksa ya da boşsa sürüm işi durur. Kullanıcıya dönük, Türkçe ve kısa
yazın; düz metin olarak gösterildiği için Markdown biçimlemesi yerine
`- ` ile başlayan maddeler kullanın.
