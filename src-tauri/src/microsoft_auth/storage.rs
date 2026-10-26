//! Large prismarine caches exceed Windows Credential Manager's blob limit.
//! Encrypt the cache with AES-GCM; store only its random key in the OS vault.
use super::{Account, runtime};
use aes_gcm::{aead::{Aead, KeyInit, Payload}, Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::fs;

const SERVICE: &str = "app.ember.launcher";
const KEY: &str = "microsoft-cache-key-v1";
const AAD: &[u8] = b"Ember Microsoft account v1";

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|e| e.to_string())
}

fn key(create: bool) -> Result<Vec<u8>, String> {
    let entry = entry(KEY)?;
    match entry.get_password() {
        Ok(text) => STANDARD.decode(text).ok().filter(|k| k.len() == 32).ok_or("Invalid Microsoft account storage key. Sign out and sign in again.".into()),
        Err(keyring::Error::NoEntry) if create => {
            let key: [u8; 32] = rand::random();
            entry.set_password(&STANDARD.encode(key)).map_err(|e| format!("Couldn't save the Microsoft account key: {e}"))?;
            Ok(key.to_vec())
        }
        Err(e) => Err(format!("Couldn't open the Microsoft account vault: {e}")),
    }
}

fn encrypt(key: &[u8], plain: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "Invalid account key")?;
    let nonce: [u8; 12] = rand::random();
    let mut data = nonce.to_vec();
    data.extend(cipher.encrypt(Nonce::from_slice(&nonce), Payload { msg: plain, aad: AAD }).map_err(|_| "Couldn't encrypt the Microsoft account")?);
    Ok(data)
}

fn decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 28 { return Err("Microsoft account cache is damaged. Sign in again.".into()) }
    Aes256Gcm::new_from_slice(key).map_err(|_| "Invalid account key")?
        .decrypt(Nonce::from_slice(&data[..12]), Payload { msg: &data[12..], aad: AAD })
        .map_err(|_| "Couldn't decrypt the Microsoft account. Sign in again.".into())
}

pub(super) fn load() -> Result<Option<Account>, String> {
    let path = runtime::directory()?.join("account.v1.enc");
    if !path.exists() {
        // Existing sessions remain usable until their MC token expires. Their
        // Azure refresh token cannot be migrated into prismarine's live flow.
        return match entry("microsoft-account")?.get_password() {
            Ok(text) => serde_json::from_str(&text).map(Some).map_err(|_| "Couldn't read the saved Microsoft account".into()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        };
    }
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
        return Err("Microsoft account cache is too large. Sign in again.".into());
    }
    let data = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&decrypt(&key(false)?, &data)?).map(Some).map_err(|_| "Couldn't read the saved Microsoft account".into())
}

pub(super) fn store(account: &Account) -> Result<(), String> {
    let data = serde_json::to_vec(account).map_err(|e| e.to_string())?;
    let encrypted = encrypt(&key(true)?, &data)?;
    let path = runtime::directory()?.join("account.v1.enc");
    let part = path.with_extension("part");
    fs::write(&part, encrypted).map_err(|e| e.to_string())?;
    fs::rename(part, path).map_err(|e| e.to_string())?;
    // Remove the old Azure refresh token only after a successful replacement.
    let _ = entry("microsoft-account")?.delete_credential();
    Ok(())
}

pub(super) fn forget() -> Result<(), String> {
    let path = runtime::directory()?.join("account.v1.enc");
    match fs::remove_file(path) {
        Ok(()) => {},
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(e.to_string()),
    }
    for name in [KEY, "microsoft-account"] {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {},
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_roundtrip_rejects_tampering_and_wrong_keys() {
        let plain = br#"{"cache":{"live":{"refresh_token":"private"}}}"#;
        let key = [7; 32];
        let mut data = encrypt(&key, plain).unwrap();
        assert_eq!(decrypt(&key, &data).unwrap(), plain);
        assert!(!data.windows(7).any(|s| s == b"private"));
        assert!(decrypt(&[8; 32], &data).is_err());
        data[20] ^= 1;
        assert!(decrypt(&key, &data).is_err());
        assert!(decrypt(&key, &[0; 12]).is_err());
    }
}
