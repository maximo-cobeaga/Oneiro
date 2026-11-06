//! Saving API keys from the interface. On Windows a key is encrypted with the user's own Windows credentials
//! (DPAPI, through PowerShell's SecureString), so the file is useless on another account or machine. On other
//! systems it is stored in a file only the user can read. Keys are never written in the project folder.

use crate::i18n::tr;
use eframe::egui::{self, RichText};
use std::path::PathBuf;

fn file(slot: &str) -> Option<PathBuf> {
    crate::welcome::config_dir().map(|d| d.join("keys").join(format!("{}.key", slot)))
}

pub fn exists(slot: &str) -> bool {
    file(slot).map_or(false, |p| p.exists())
}

/// Encrypts / decrypts with the current Windows user's credentials (DPAPI).
#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
        unsafe { LocalFree(out.pbData as _) };
        v
    }

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(plain);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
        if ok == 0 {
            return Err("Windows could not encrypt the key".into());
        }
        Ok(take(out))
    }

    pub fn unprotect(cipher: &[u8]) -> Option<Vec<u8>> {
        let input = blob(cipher);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
        if ok == 0 {
            return None;
        }
        Some(take(out))
    }
}

#[cfg(windows)]
fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

#[cfg(windows)]
fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// Saves `key` for `slot` (encrypted on Windows).
pub fn save(slot: &str, key: &str) -> Result<(), String> {
    let path = file(slot).ok_or("no place to save the key")?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let key = key.trim();
    if key.is_empty() {
        return Err("the key is empty".into());
    }
    #[cfg(windows)]
    let data = to_hex(&dpapi::protect(key.as_bytes())?);
    #[cfg(not(windows))]
    let data = key.to_string();
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn load(slot: &str) -> Option<String> {
    let path = file(slot)?;
    let data = std::fs::read_to_string(&path).ok()?;
    let data = data.trim();
    if data.is_empty() {
        return None;
    }
    #[cfg(windows)]
    {
        let plain = dpapi::unprotect(&from_hex(data)?)?;
        String::from_utf8(plain).ok().filter(|k| !k.is_empty())
    }
    #[cfg(not(windows))]
    {
        Some(data.to_string())
    }
}

pub fn forget(slot: &str) {
    if let Some(p) = file(slot) {
        let _ = std::fs::remove_file(p);
    }
}

/// A key field with Save and Forget. `msg` receives the outcome.
pub fn key_row(ui: &mut egui::Ui, key: &mut String, slot: &str, msg: &mut String) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(key).password(true).desired_width(170.0).hint_text(tr("paste your key here")));
        if ui.add_enabled(!key.trim().is_empty(), egui::Button::new(tr("Save key"))).on_hover_text(tr("Remember it on this computer (encrypted for your Windows user)")).clicked() {
            *msg = match save(slot, key) {
                Ok(()) => tr("Key saved. It is loaded every time you open Oneiro.").to_string(),
                Err(e) => e,
            };
        }
        if ui.add_enabled(exists(slot), egui::Button::new(tr("Forget"))).clicked() {
            forget(slot);
            key.clear();
            *msg = tr("Saved key removed.").to_string();
        }
    });
    ui.label(RichText::new(if exists(slot) { tr("A key is saved on this computer.") } else { tr("Not saved: it is lost when you close Oneiro unless you press Save key.") }).weak().small());
    if !msg.is_empty() {
        ui.label(RichText::new(msg.as_str()).small());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_key_comes_back_and_can_be_forgotten() {
        let _g = crate::welcome::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("oneiro_keys_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("ONEIRO_CONFIG_DIR", &dir);
        assert!(!exists("t"));
        save("t", "  sk-test-123  ").unwrap();
        assert!(exists("t"));
        assert_eq!(load("t").as_deref(), Some("sk-test-123"));
        #[cfg(windows)]
        assert!(!std::fs::read_to_string(file("t").unwrap()).unwrap().contains("sk-test-123"), "encrypted on disk");
        forget("t");
        assert!(!exists("t") && load("t").is_none());
        assert!(save("t", "   ").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
