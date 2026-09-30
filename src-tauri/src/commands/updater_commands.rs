//! Otomatik güncelleme desteğinin çalışma zamanı kararı.
//!
//! Güncellemeyi indirme/kurma işini arayüz `tauri-plugin-updater` ile yapar;
//! bu komut yalnızca "bu kurulumda uygulama kendini güncellemeli mi" sorusunu
//! yanıtlar. Yanlış yerde açık bırakılan güncelleyici, paket yöneticisinin ya
//! da Microsoft Store'un yönettiği bir kurulumu bozar.

/// Karar girdileri; saf fonksiyon ortamı doğrudan okumasın, test edilebilsin.
#[derive(Debug, Clone, Copy)]
struct UpdaterEnvironment<'a> {
    /// `cfg!(debug_assertions)`: geliştirme derlemesi kurulu bir uygulama değil,
    /// imzalı bir yayın da yok.
    is_debug_build: bool,
    /// `std::env::consts::OS` değeri.
    os: &'a str,
    /// Windows'ta MSIX paket kimliğiyle çalışıyor mu.
    is_msix_packaged: bool,
    /// Linux'ta `APPIMAGE` ortam değişkeni tanımlı mı.
    is_appimage: bool,
}

fn decide_updater_supported(env: UpdaterEnvironment) -> bool {
    if env.is_debug_build {
        return false;
    }
    match env.os {
        // MSIX'i Microsoft Store günceller; uygulama içi güncelleme paket
        // bütünlüğünü bozar. NSIS/MSI kurulumu güncelleyiciyle yenilenir.
        "windows" => !env.is_msix_packaged,
        // .deb/.rpm paket yöneticisine aittir; yalnız AppImage kendini
        // değiştirebilir (güncelleyici çalışan AppImage dosyasının yerine yazar).
        "linux" => env.is_appimage,
        "macos" => true,
        _ => false,
    }
}

/// Windows'ta süreç MSIX paket kimliğiyle mi çalışıyor. `GetCurrentPackageFullName`
/// tampon verilmeden çağrılır: paket yoksa `APPMODEL_ERROR_NO_PACKAGE` döner,
/// paket varsa `ERROR_INSUFFICIENT_BUFFER` (yani "paketli").
#[cfg(windows)]
fn is_msix_packaged() -> bool {
    use windows_sys::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

    let mut length: u32 = 0;
    // SAFETY: `length` geçerli bir yerel değişkendir; tampon işaretçisi boş
    // verilir ve API bu durumda yalnızca gereken uzunluğu/durum kodunu yazar.
    let status = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
    status != APPMODEL_ERROR_NO_PACKAGE
}

#[cfg(not(windows))]
fn is_msix_packaged() -> bool {
    false
}

/// Arayüz bu değere bakarak güncelleme denetimini açar ya da hiç göstermez.
#[tauri::command]
pub fn updater_supported() -> bool {
    decide_updater_supported(UpdaterEnvironment {
        is_debug_build: cfg!(debug_assertions),
        os: std::env::consts::OS,
        is_msix_packaged: is_msix_packaged(),
        is_appimage: std::env::var_os("APPIMAGE").is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(os: &str, is_msix_packaged: bool, is_appimage: bool) -> bool {
        decide_updater_supported(UpdaterEnvironment { is_debug_build: false, os, is_msix_packaged, is_appimage })
    }

    #[test]
    fn debug_build_is_never_supported() {
        for os in ["windows", "macos", "linux"] {
            let env = UpdaterEnvironment { is_debug_build: true, os, is_msix_packaged: false, is_appimage: true };
            assert!(!decide_updater_supported(env), "{os}");
        }
    }

    #[test]
    fn windows_installer_is_supported_but_msix_is_not() {
        assert!(release("windows", false, false));
        assert!(!release("windows", true, false));
    }

    #[test]
    fn linux_is_supported_only_as_appimage() {
        assert!(release("linux", false, true));
        assert!(!release("linux", false, false));
    }

    #[test]
    fn macos_is_supported() {
        assert!(release("macos", false, false));
    }

    #[test]
    fn unknown_os_is_not_supported() {
        assert!(!release("freebsd", false, true));
    }
}
