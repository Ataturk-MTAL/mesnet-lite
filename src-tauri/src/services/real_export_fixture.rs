//! Test yardımcısı: depo kökündeki `data/` klasöründeki GERÇEK örnek dışa
//! aktarım dosyalarını bulur.
//!
//! Gerçek JotForm/e-Okul dışa aktarımları öğrenci kişisel verisi içerdiği
//! için depoya commit edilmez (`.gitignore`: `/data`). Bu yüzden bu
//! dosyalara bağlı testler dosya yoksa ATLANIR — ama atlama sessiz
//! olmamalı: `eprintln!` ile açıkça bildirilir, aksi halde "ok" çıktısı
//! testin gerçekten koştuğu izlenimini verir. Bu projede daha önce hiç
//! çağrılmayan mevzuat kontrolleri yüzünden kurallar fiilen uygulanmamıştı;
//! aynı tuzağa CSV/XLS testlerinde de düşmeyelim.
//!
//! `csv_import.rs` ve `import_apply.rs` aynı gerçek JotForm CSV'sine karşı
//! test yazdığı için arama mantığı burada tek yerde toplanır (DRY).

use std::path::PathBuf;

/// Depo kökündeki `data/` klasörünün yolu. `src-tauri`'nin bir üst
/// dizinidir; `CARGO_MANIFEST_DIR` her zaman `src-tauri`'yi gösterdiği için
/// bu hesaplama derleme zamanında sabittir.
fn data_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri'nin bir üst dizini olmalı")
        .join("data")
}

/// `data/` klasöründe verilen uzantıya sahip ilk dosyayı bulur. Dosya yoksa
/// `None` döner ve `skip_message`'ı `eprintln!` ile yazar ki test "ok"
/// çıktısı verirken aslında hiçbir şey doğrulamadığı gizli kalmasın.
pub(crate) fn find_real_export(extension: &str, skip_message: &str) -> Option<PathBuf> {
    let found = std::fs::read_dir(data_dir()).ok().and_then(|entries| {
        entries
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension)))
    });

    if found.is_none() {
        eprintln!("{skip_message}");
    }

    found
}
