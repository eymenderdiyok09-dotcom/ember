//! Microsoft → Xbox → Minecraft authentication through prismarine-auth.
//! Only device codes and public profile details cross the frontend boundary.
mod runtime;
mod storage;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{io::{BufRead, BufReader, Read, Write}, process::{Child, Command, Stdio},
    sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Mutex}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use tauri::Emitter;

const HELPER: &str = include_str!("../../auth-helper/dist/main.cjs");

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    pub xuid: String,
    pub mc_token: String,
    pub mc_expires_at: u64,
    pub skin_url: Option<String>,
    #[serde(default)]
    pub cache: Value,
}

#[derive(Serialize)]
pub struct AccountInfo {
    pub name: String,
    pub uuid: String,
    pub skin_url: Option<String>,
}

impl From<&Account> for AccountInfo {
    fn from(a: &Account) -> Self {
        Self { name: a.name.clone(), uuid: a.uuid.clone(), skin_url: a.skin_url.clone() }
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct DeviceCode {
    user_code: String,
    verification_uri: String,
    expires_in: u64,
}

struct Job {
    attempt: String,
    cancel: Arc<AtomicBool>,
    verification_uri: Option<String>,
}

static PENDING: Mutex<Option<Job>> = Mutex::new(None);

struct LoginGuard { attempt: String, cancel: Arc<AtomicBool> }
impl LoginGuard {
    fn start(attempt: String, replace: bool) -> Result<Self, String> {
        let mut pending = PENDING.lock().unwrap();
        if pending.is_some() && !replace { return Err("Finish Microsoft sign-in before starting the game.".into()) }
        if let Some(old) = pending.take() { old.cancel.store(true, Ordering::SeqCst); }
        let cancel = Arc::new(AtomicBool::new(false));
        *pending = Some(Job { attempt: attempt.clone(), cancel: cancel.clone(), verification_uri: None });
        Ok(Self { attempt, cancel })
    }

    fn with_current<T>(&self, action: impl FnOnce(&mut Job) -> Result<T, String>) -> Result<T, String> {
        let mut pending = PENDING.lock().unwrap();
        let job = pending.as_mut().filter(|p| p.attempt == self.attempt && Arc::ptr_eq(&p.cancel, &self.cancel))
            .ok_or("Sign-in cancelled")?;
        runtime::check_cancel(&self.cancel)?;
        action(job)
    }
}
impl Drop for LoginGuard {
    fn drop(&mut self) {
        let mut pending = PENDING.lock().unwrap();
        if pending.as_ref().is_some_and(|p| Arc::ptr_eq(&p.cancel, &self.cancel)) { *pending = None; }
    }
}

struct Helper(Child);
impl Drop for Helper {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) }

fn validate_code(code: &DeviceCode) -> Result<(), String> {
    let url = tauri::Url::parse(&code.verification_uri).map_err(|_| "Invalid Microsoft verification URL")?;
    if url.scheme() != "https" || !matches!(url.host_str(), Some("microsoft.com" | "www.microsoft.com" | "login.microsoftonline.com"))
        || !url.username().is_empty() || url.password().is_some() || url.port().is_some()
        || !(4..=20).contains(&code.user_code.len())
        || !code.user_code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
        || !(1..=1800).contains(&code.expires_in) {
        return Err("Invalid Microsoft sign-in code".into());
    }
    Ok(())
}

fn helper_error(code: &str) -> String {
    match code {
        "SIGN_IN_REQUIRED" => "Your Microsoft sign-in expired. Sign in again.",
        "PROFILE_UNAVAILABLE" => "Couldn't load your Minecraft profile. Make sure this account owns Java Edition and has a profile at minecraft.net, then try again.",
        "2148916233" => "This account has no Xbox profile. Sign in at xbox.com to create one, then try again.",
        "2148916235" => "Xbox Live isn't available in your country.",
        "2148916236" | "2148916237" => "This account needs adult verification at xbox.com first.",
        "2148916238" => "An adult needs to add this account to a Microsoft family before it can use Xbox Live.",
        "TIMEOUT" => "Microsoft sign-in timed out. Try again.",
        "NETWORK" => "Couldn't reach Microsoft or Minecraft. Check your internet connection and try again.",
        "FORBIDDEN" => "Microsoft or Minecraft refused sign-in. Check your account at minecraft.net and try again.",
        _ => "Microsoft sign-in failed. Please try again.",
    }.into()
}

fn run_helper(guard: &LoginGuard, cache: Option<Value>, on_code: impl Fn(DeviceCode) -> Result<(), String>) -> Result<Account, String> {
    let node = runtime::node(&guard.cancel)?;
    let script = runtime::helper(HELPER)?;
    runtime::check_cancel(&guard.cancel)?;
    let refreshing = cache.is_some();
    let mut command = Command::new(node);
    command.arg(script).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .env_remove("NODE_OPTIONS").env_remove("NODE_PATH").env_remove("DEBUG").env_remove("SSLKEYLOGFILE");
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = Helper(command.spawn().map_err(|e| format!("Couldn't start Microsoft sign-in: {e}"))?);
    // Keep stdin open while the helper runs; closing it tells Node its parent exited.
    let mut input = child.0.stdin.take().ok_or("Couldn't open the sign-in helper")?;
    let request = json!({ "mode": if refreshing { "refresh" } else { "login" }, "cache": cache.unwrap_or_else(|| json!({})) });
    serde_json::to_writer(&mut input, &request).map_err(|_| "Couldn't send data to the sign-in helper")?;
    input.write_all(b"\n").map_err(|_| "Couldn't start the sign-in helper")?;
    let stdout = child.0.stdout.take().ok_or("Couldn't read the sign-in helper")?;
    let (send, receive) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            match (&mut reader).take(4 * 1024 * 1024).read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if line.last() != Some(&b'\n') { break }
                    let parsed = serde_json::from_slice::<Value>(&line).map_err(|_| "Invalid response from the sign-in helper".to_owned());
                    if send.send(parsed).is_err() { break }
                }
            }
        }
    });
    let mut deadline = Instant::now() + Duration::from_secs(if refreshing { 120 } else { 900 });
    loop {
        runtime::check_cancel(&guard.cancel)?;
        if Instant::now() >= deadline { return Err(helper_error("TIMEOUT")) }
        let message = match receive.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => message?,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return Err("The Microsoft sign-in helper stopped unexpectedly. Try again.".into()),
        };
        match message["type"].as_str() {
            Some("device_code") => {
                if refreshing { return Err(helper_error("SIGN_IN_REQUIRED")) }
                let code: DeviceCode = serde_json::from_value(message).map_err(|_| "Invalid Microsoft sign-in code")?;
                validate_code(&code)?;
                deadline = Instant::now() + Duration::from_secs(code.expires_in.min(900));
                on_code(code)?;
            }
            Some("account") => {
                let account: Account = serde_json::from_value(message["account"].clone()).map_err(|_| "Invalid Minecraft account response")?;
                if account.mc_token.is_empty() || account.mc_expires_at <= now() + 60 || account.uuid.len() != 32
                    || !account.uuid.chars().all(|c| c.is_ascii_hexdigit()) || account.name.is_empty() || !account.cache.is_object() {
                    return Err("Invalid Minecraft account response".into());
                }
                runtime::check_cancel(&guard.cancel)?;
                return Ok(account);
            }
            Some("error") => return Err(helper_error(message["code"].as_str().unwrap_or("AUTH_FAILED"))),
            _ => return Err("Unexpected response from the sign-in helper".into()),
        }
    }
}

#[tauri::command]
pub fn msa_configured() -> bool { runtime::supported() }

#[tauri::command]
pub fn msa_status() -> Option<AccountInfo> { storage::load().ok().flatten().as_ref().map(AccountInfo::from) }

/// Emits only the device code, then returns the public account after sign-in.
#[tauri::command]
pub async fn msa_begin_login(app: tauri::AppHandle, attempt: String) -> Result<AccountInfo, String> {
    if attempt.is_empty() || attempt.len() > 64 { return Err("Invalid sign-in attempt".into()) }
    let guard = LoginGuard::start(attempt, true)?;
    tauri::async_runtime::spawn_blocking(move || {
        let account = run_helper(&guard, None, |code| {
            guard.with_current(|job| {
                job.verification_uri = Some(code.verification_uri.clone());
                app.emit("msa-device-code", json!({ "attempt": job.attempt, "code": code })).map_err(|e| e.to_string())
            })
        })?;
        guard.with_current(|_| { storage::store(&account)?; Ok(AccountInfo::from(&account)) })
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn msa_open_verification(attempt: String) -> Result<(), String> {
    let pending = PENDING.lock().unwrap();
    let job = pending.as_ref().filter(|p| p.attempt == attempt).ok_or("No sign-in in progress")?;
    open_url(job.verification_uri.as_deref().ok_or("Microsoft hasn't sent a sign-in code yet")?)
}

/// Also used by Ely.by and the mod browser for ordinary HTTPS links.
pub(crate) fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") { return Err("Unexpected sign-in URL".into()) }
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let result = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = Command::new("xdg-open").arg(url).status();
    match result { Ok(status) if status.success() => Ok(()), _ => Err("Couldn't open your browser. Open the sign-in page manually.".into()) }
}

#[tauri::command]
pub fn msa_cancel_login(attempt: String) {
    let mut pending = PENDING.lock().unwrap();
    if pending.as_ref().is_some_and(|p| p.attempt == attempt) {
        if let Some(job) = pending.take() { job.cancel.store(true, Ordering::SeqCst); }
    }
}

#[tauri::command]
pub fn msa_logout() -> Result<(), String> {
    let mut pending = PENDING.lock().unwrap();
    if let Some(job) = pending.take() { job.cancel.store(true, Ordering::SeqCst); }
    storage::forget()
}

pub(crate) fn session() -> Result<Account, String> {
    let guard = LoginGuard::start(format!("refresh-{}", rand::random::<u64>()), false)?;
    let account = storage::load()?.ok_or("Sign in with Microsoft to play online.")?;
    if account.mc_expires_at > now() + 600 { return guard.with_current(|_| Ok(account)) }
    if !account.cache.is_object() { return Err(helper_error("SIGN_IN_REQUIRED")) }
    let refreshed = run_helper(&guard, Some(account.cache), |_| Err(helper_error("SIGN_IN_REQUIRED")))?;
    guard.with_current(|_| { storage::store(&refreshed)?; Ok(refreshed) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_codes_and_rejects_untrusted_browser_destinations() {
        let mut code = DeviceCode { user_code: "EMBR-7Q2K".into(), verification_uri: "https://www.microsoft.com/link".into(), expires_in: 900 };
        assert!(validate_code(&code).is_ok());
        for url in ["http://microsoft.com/link", "https://microsoft.com.attacker.test/", "https://user@microsoft.com/link", "https://microsoft.com:8443/link", "file:///tmp/test"] {
            code.verification_uri = url.into();
            assert!(validate_code(&code).is_err());
        }
    }
    #[test]
    fn legacy_account_can_be_read_but_needs_reauthentication_to_refresh() {
        let account: Account = serde_json::from_value(json!({ "name": "Player", "uuid": "abcd", "xuid": "0", "mc_token": "old", "mc_expires_at": 1, "skin_url": null, "refresh_token": "legacy" })).unwrap();
        assert!(account.cache.is_null());
        let public = serde_json::to_value(AccountInfo::from(&account)).unwrap();
        assert_eq!(public.as_object().unwrap().len(), 3);
        assert!(public.get("mc_token").is_none());
    }
    #[test]
    fn cancelling_old_attempt_cannot_cancel_or_clear_new_attempt() {
        let first = LoginGuard::start("first".into(), true).unwrap();
        let second = LoginGuard::start("second".into(), true).unwrap();
        assert!(first.cancel.load(Ordering::SeqCst));
        msa_cancel_login("first".into());
        drop(first);
        assert!(second.with_current(|_| Ok(())).is_ok());
        msa_cancel_login("second".into());
        assert!(second.with_current(|_| Ok(())).is_err());
    }

    #[test]
    #[ignore = "requests a Microsoft device code, then cancels without signing in"]
    fn prismarine_device_code_smoke() {
        let guard = LoginGuard::start("smoke".into(), true).unwrap();
        let seen = AtomicBool::new(false);
        let result = run_helper(&guard, None, |code| {
            validate_code(&code)?;
            seen.store(true, Ordering::SeqCst);
            guard.cancel.store(true, Ordering::SeqCst);
            Ok(())
        });
        assert!(seen.load(Ordering::SeqCst), "No valid Microsoft device code: {}", result.err().unwrap_or_default());
        assert!(guard.cancel.load(Ordering::SeqCst));
    }
}
