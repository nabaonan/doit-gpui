//! WebDAV 密码的安全存储。
//!
//! 密码绝不明文落盘：本地保存与云端快照中的 `webdav_password` 一律是密文。
//! 这里用 AES-256-GCM（AEAD，机密性 + 防篡改）加密，密钥是本机随机生成的
//! 256 位设备级密钥，单独存于 `~/.doit-gpui/secret.key`（0600 权限）。
//!
//! 存储格式（UTF-8 字符串，可直接放 JSON 字段）：
//!   `enc:<base64 nonce>:<base64 ciphertext+tag>`
//! 带 `enc:` 前缀用于区分"已加密密文"与历史遗留的明文。

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::Engine as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const PREFIX: &str = "enc:";
const KEY_DIR: &str = ".doit-gpui";
const KEY_FILE: &str = "secret.key";

/// The encryption key, loaded once per process. Cached as `Result` so a
/// persistent key-file problem (e.g. corrupt file) surfaces the same way every
/// call, instead of racing a re-read; a re-run of the app re-initializes it.
static KEY: OnceLock<Result<[u8; 32], String>> = OnceLock::new();

fn key_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(KEY_DIR).join(KEY_FILE)
}

/// Load the machine-local key, generating and storing a fresh random one on
/// first use. Errors are treated as "no key available" so the UI can surface a
/// clear message instead of crashing. Initialization is atomic (`OnceLock`), so
/// concurrent first calls — e.g. UI and background threads — can never disagree
/// about which key is in effect.
fn load_or_create_key() -> Result<[u8; 32], String> {
    KEY.get_or_init(|| {
        let path = key_path();
        if path.exists() {
            let bytes = std::fs::read(&path).map_err(|e| format!("读取加密密钥失败：{e}"))?;
            if bytes.len() != 32 {
                return Err(format!(
                    "加密密钥文件损坏（长度 {}，期望 32 字节），请删除 {} 后重试",
                    bytes.len(),
                    path.display()
                ));
            }
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            return Ok(k);
        }
        let mut k = [0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut k);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, &k);
        // Key file is private to this user.
        let _ = set_private(&path);
        Ok::<[u8; 32], String>(k)
    })
    .as_ref()
    .map_err(|e| e.clone())
    .copied()
}

/// Best-effort chmod 0600 on the key file (macOS/Linux).
#[cfg(unix)]
fn set_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Encrypt `plain` into the `enc:` payload. Errors surface to the caller so the
/// UI can explain why saving failed rather than silently storing plaintext.
pub fn encrypt(plain: &str) -> Result<String, String> {
    if plain.is_empty() {
        return Ok(String::new());
    }
    let key = load_or_create_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("初始化 AES-GCM 失败：{e}"))?;
    let mut nonce_bytes = [0u8; 12];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, Payload { msg: plain.as_bytes(), aad: b"doit-webdav" })
        .map_err(|e| format!("加密失败：{e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD;
    Ok(format!(
        "{PREFIX}{}:{}",
        b64.encode(nonce_bytes),
        b64.encode(ciphertext)
    ))
}

/// Decrypt an `enc:` payload back to plaintext. Returns `None` when the value
/// is empty, undecodable, or (importantly) a legacy plaintext with no `enc:`
/// prefix — callers then know the configured value is not usable as-is.
pub fn decrypt(encoded: &str) -> Option<String> {
    if encoded.is_empty() {
        return Some(String::new());
    }
    let payload = encoded.strip_prefix(PREFIX)?;
    let (nonce_b64, ct_b64) = payload.split_once(':')?;
    let b64 = base64::engine::general_purpose::STANDARD;

    let nonce_bytes = b64.decode(nonce_b64).ok()?;
    if nonce_bytes.len() != 12 {
        return None;
    }
    let ct = b64.decode(ct_b64).ok()?;

    let key = load_or_create_key().ok()?;
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), Payload { msg: &ct, aad: b"doit-webdav" })
        .ok()?;
    String::from_utf8(plain).ok()
}

/// Whether a stored value is encrypted (has our `enc:` prefix).
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// Best-effort: make sure the key exists so later encrypt() calls won't fail.
/// Called once at startup to surface storage problems early.
pub fn ensure_key() -> Result<(), String> {
    let _ = load_or_create_key()?;
    Ok(())
}

/// Encrypt a freshly typed password for storage; never falls back to
/// plaintext. On failure returns an error so the UI can refuse to save.
pub fn encrypt_or_error(plain: &str) -> Result<String, String> {
    if plain.is_empty() {
        return Ok(String::new());
    }
    // Already-encrypted values (e.g. re-read from settings) pass through.
    if is_encrypted(plain) {
        return Ok(plain.to_string());
    }
    encrypt(plain)
}

/// Decrypt a stored password for display in an input. Legacy plaintext (from
/// snapshots saved before encryption existed) passes through unchanged so the
/// user keeps working, and the next save re-encrypts it; corrupt ciphertext
/// comes back empty so the user re-enters rather than seeing garbage.
pub fn decrypt_for_display(stored: &str) -> String {
    if stored.is_empty() {
        return String::new();
    }
    match decrypt(stored) {
        Some(plain) => plain,
        None if !is_encrypted(stored) => stored.to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let plain = "s3cret-密码-🚀";
        let enc = encrypt(plain).unwrap();
        assert!(enc.starts_with(PREFIX));
        assert_ne!(enc, plain);
        assert_eq!(decrypt(&enc).as_deref(), Some(plain));
    }

    #[test]
    fn decrypt_rejects_legacy_plaintext() {
        // 旧版本存的是明文，无 enc: 前缀 → 视为不可解密。
        assert_eq!(decrypt("mypassword"), None);
        assert_eq!(decrypt(""), Some(String::new()));
    }

    #[test]
    fn display_keeps_legacy_plaintext_but_roundtrips_ciphertext() {
        // 升级前的明文密码原样显示，保证老用户不断档。
        assert_eq!(decrypt_for_display("mypassword"), "mypassword");
        assert_eq!(decrypt_for_display(""), "");
        // 加密后的值还原为明文。
        let enc = encrypt("s3cret").unwrap();
        assert_eq!(decrypt_for_display(&enc), "s3cret");
        // 损坏的密文回到空串，而不是把乱码当密码。
        assert_eq!(decrypt_for_display("enc:AAAA:BBBB"), "");
    }

    #[test]
    fn encryption_is_randomized() {
        // 同一个明文两次加密，nonce 不同 → 密文不同。
        let a = encrypt("same").unwrap();
        let b = encrypt("same").unwrap();
        assert_ne!(a, b);
        assert_eq!(decrypt(&a).as_deref(), Some("same"));
        assert_eq!(decrypt(&b).as_deref(), Some("same"));
    }
}
