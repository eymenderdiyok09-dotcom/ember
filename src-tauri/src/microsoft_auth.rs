//! Microsoft account sign-in for online play.
//!
//! Device-code OAuth with Microsoft → Xbox Live → XSTS → Minecraft services.
//! The refresh token and Minecraft token are kept in the OS credential store
//! (macOS Keychain / Windows Credential Manager), never in plain files.
//!
//! Needs an Azure app registration (public client, personal accounts) whose
//! client ID is approved by Mojang for Minecraft services; set it at build
//! time with `EMBER_MSA_CLIENT_ID`.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CLIENT_ID: Option<&str> = option_env!("EMBER_MSA_CLIENT_ID");
const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const KEYRING_SERVICE: &str = "app.ember.launcher";
const KEYRING_USER: &str = "microsoft-account";

/// Stored in the credential store.
#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    pub xuid: String,
    pub mc_token: String,
    /// Unix seconds.
    pub mc_expires_at: u64,
    pub refresh_token: String,
    pub skin_url: Option<String>,
}

/// What the UI is allowed to see.
#[derive(Serialize)]
pub struct AccountInfo {
    pub name: String,
    pub uuid: String,
    pub skin_url: Option<String>,
}

impl From<&Account> for AccountInfo {
    fn from(a: &Account) -> Self {
        AccountInfo { name: a.name.clone(), uuid: a.uuid.clone(), skin_url: a.skin_url.clone() }
    }
}

#[derive(Serialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

struct PendingLogin {
    device_code: String,
    verification_uri: String,
    interval: u64,
    expires_at: Instant,
}

static PENDING: Mutex<Option<PendingLogin>> = Mutex::new(None);
static CANCELLED: AtomicBool = AtomicBool::new(false);

fn client_id() -> Result<&'static str, String> {
    CLIENT_ID.filter(|id| !id.is_empty()).ok_or_else(|| {
        "Microsoft sign-in isn't set up in this build of Ember yet (no Azure client ID).".into()
    })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .user_agent(concat!("Ember/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Sends a request and returns (status, JSON body); non-JSON bodies become `null`.
fn read(response: Result<ureq::http::Response<ureq::Body>, ureq::Error>, what: &str) -> Result<(u16, Value), String> {
    let mut response = response.map_err(|e| format!("{what}: {e}"))?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_json::<Value>().unwrap_or(Value::Null);
    Ok((status, body))
}

fn post_form(agent: &ureq::Agent, url: &str, form: &[(&str, &str)], what: &str) -> Result<(u16, Value), String> {
    read(agent.post(url).header("Accept", "application/json").send_form(form.iter().copied()), what)
}

fn post_json(agent: &ureq::Agent, url: &str, body: &Value, what: &str) -> Result<(u16, Value), String> {
    read(agent.post(url).header("Accept", "application/json").send_json(body), what)
}

// ---------- credential store ----------

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

fn load() -> Option<Account> {
    serde_json::from_str(&entry().ok()?.get_password().ok()?).ok()
}

fn store(account: &Account) -> Result<(), String> {
    let text = serde_json::to_string(account).map_err(|e| e.to_string())?;
    entry()?.set_password(&text).map_err(|e| format!("Couldn't save the account: {e}"))
}

// ---------- commands ----------

/// Whether this build can sign in (it was built with a client ID).
#[tauri::command]
pub fn msa_configured() -> bool {
    client_id().is_ok()
}

/// The signed-in account, if any (no network).
#[tauri::command]
pub fn msa_status() -> Option<AccountInfo> {
    load().as_ref().map(AccountInfo::from)
}

/// Starts a device-code sign-in; the user enters the code at `verification_uri`.
#[tauri::command]
pub async fn msa_begin_login() -> Result<DeviceCode, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let client_id = client_id()?;
        let (status, body) =
            post_form(&agent(), DEVICE_CODE_URL, &[("client_id", client_id), ("scope", SCOPE)], "Microsoft sign-in")?;
        if status != 200 {
            return Err(oauth_error("Microsoft sign-in", &body));
        }
        let field = |k: &str| body[k].as_str().map(str::to_owned).ok_or_else(|| format!("Microsoft sign-in: missing {k}"));
        let expires_in = body["expires_in"].as_u64().unwrap_or(900);
        let code = DeviceCode {
            user_code: field("user_code")?,
            verification_uri: field("verification_uri")?,
            expires_in,
        };
        CANCELLED.store(false, Ordering::Relaxed);
        *PENDING.lock().unwrap() = Some(PendingLogin {
            device_code: field("device_code")?,
            verification_uri: code.verification_uri.clone(),
            interval: body["interval"].as_u64().unwrap_or(5).max(1),
            expires_at: Instant::now() + Duration::from_secs(expires_in),
        });
        Ok(code)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens Microsoft's sign-in page for the pending login in the default browser.
#[tauri::command]
pub fn msa_open_verification() -> Result<(), String> {
    let url = PENDING
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.verification_uri.clone())
        .ok_or("No sign-in in progress")?;
    open_url(&url)
}

/// Opens an https URL in the default browser.
pub(crate) fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Unexpected sign-in URL".into());
    }
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(url).status();
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Waits for the user to finish signing in, then signs in to Minecraft.
#[tauri::command]
pub async fn msa_finish_login() -> Result<AccountInfo, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let client_id = client_id()?;
        let (device_code, mut interval, expires_at) = {
            let pending = PENDING.lock().unwrap();
            let p = pending.as_ref().ok_or("No sign-in in progress")?;
            (p.device_code.clone(), p.interval, p.expires_at)
        };
        let agent = agent();
        let tokens = loop {
            for _ in 0..interval * 4 {
                if CANCELLED.load(Ordering::Relaxed) {
                    return Err("Sign-in cancelled".into());
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            if Instant::now() > expires_at {
                return Err("The sign-in code expired. Try again.".into());
            }
            let (status, body) = post_form(
                &agent,
                TOKEN_URL,
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", client_id),
                    ("device_code", &device_code),
                ],
                "Microsoft sign-in",
            )?;
            if status == 200 {
                break body;
            }
            match body["error"].as_str() {
                Some("authorization_pending") => {}
                Some("slow_down") => interval += 5,
                Some("authorization_declined") => return Err("Sign-in was declined.".into()),
                Some("expired_token") => return Err("The sign-in code expired. Try again.".into()),
                _ => return Err(oauth_error("Microsoft sign-in", &body)),
            }
        };
        *PENDING.lock().unwrap() = None;
        let account = minecraft_login(&agent, &tokens)?;
        store(&account)?;
        Ok(AccountInfo::from(&account))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn msa_cancel_login() {
    CANCELLED.store(true, Ordering::Relaxed);
    *PENDING.lock().unwrap() = None;
}

#[tauri::command]
pub fn msa_logout() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// A signed-in account with a Minecraft token valid for at least 10 minutes.
pub(crate) fn session() -> Result<Account, String> {
    let account = load().ok_or("Sign in with Microsoft to play online.")?;
    if account.mc_expires_at > now() + 600 {
        return Ok(account);
    }
    let client_id = client_id()?;
    let agent = agent();
    let (status, tokens) = post_form(
        &agent,
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", &account.refresh_token),
            ("scope", SCOPE),
        ],
        "Refreshing Microsoft sign-in",
    )?;
    if status != 200 {
        return Err("Your Microsoft sign-in expired. Sign in again.".into());
    }
    let account = minecraft_login(&agent, &tokens)?;
    store(&account)?;
    Ok(account)
}

// ---------- token chain ----------

fn oauth_error(what: &str, body: &Value) -> String {
    let detail = body["error_description"]
        .as_str()
        .and_then(|d| d.lines().next())
        .or(body["error"].as_str())
        .unwrap_or("unexpected response");
    format!("{what}: {detail}")
}

/// Microsoft OAuth tokens → Minecraft account.
fn minecraft_login(agent: &ureq::Agent, ms_tokens: &Value) -> Result<Account, String> {
    let ms_access = ms_tokens["access_token"].as_str().ok_or("Microsoft returned no access token")?;
    let refresh_token = ms_tokens["refresh_token"].as_str().ok_or("Microsoft returned no refresh token")?;

    let (status, xbl) = post_json(
        agent,
        XBL_URL,
        &json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}") },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }),
        "Xbox Live sign-in",
    )?;
    let xbl_token = xbl["Token"].as_str().filter(|_| status == 200).ok_or("Xbox Live sign-in failed")?;
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or("Xbox Live returned no user hash")?;

    let (status, xsts) = post_json(
        agent,
        XSTS_URL,
        &json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }),
        "Xbox Live authorization",
    )?;
    if status != 200 {
        return Err(match xsts["XErr"].as_u64() {
            Some(2148916233) => "This Microsoft account has no Xbox profile. Sign in once at minecraft.net to create one.".into(),
            Some(2148916235) => "Xbox Live isn't available in your country.".into(),
            Some(2148916236 | 2148916237) => "This account needs adult verification on xbox.com first.".into(),
            Some(2148916238) => "This is a child account. An adult must add it to a Microsoft family first.".into(),
            _ => format!("Xbox Live authorization failed (HTTP {status})."),
        });
    }
    let xsts_token = xsts["Token"].as_str().ok_or("Xbox Live returned no XSTS token")?;

    let (status, mc) = post_json(
        agent,
        MC_LOGIN_URL,
        &json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }),
        "Minecraft sign-in",
    )?;
    if status == 403 {
        return Err("Minecraft refused this app. Its Azure client ID must be approved by Mojang (aka.ms/mce-reviewappid).".into());
    }
    let mc_token = mc["access_token"].as_str().filter(|_| status == 200).ok_or_else(|| format!("Minecraft sign-in failed (HTTP {status})."))?;
    let expires_in = mc["expires_in"].as_u64().unwrap_or(86400);

    let (status, profile) = read(
        agent.get(MC_PROFILE_URL).header("Authorization", &format!("Bearer {mc_token}")).call(),
        "Minecraft profile",
    )?;
    if status == 404 {
        return Err("This Microsoft account doesn't own Minecraft: Java Edition.".into());
    }
    if status != 200 {
        return Err(format!("Couldn't load the Minecraft profile (HTTP {status})."));
    }
    let skin_url = profile["skins"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["state"] == "ACTIVE")
        .and_then(|s| s["url"].as_str())
        .map(|u| u.replacen("http://", "https://", 1));

    Ok(Account {
        name: profile["name"].as_str().ok_or("profile has no name")?.to_owned(),
        uuid: profile["id"].as_str().ok_or("profile has no id")?.to_owned(),
        xuid: jwt_claim(mc_token, "xuid").unwrap_or_else(|| "0".into()),
        mc_token: mc_token.to_owned(),
        mc_expires_at: now() + expires_in,
        refresh_token: refresh_token.to_owned(),
        skin_url,
    })
}

/// Reads a string claim from a JWT payload without verifying it (it's only
/// passed back to the game).
fn jwt_claim(token: &str, claim: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let json: Value = serde_json::from_slice(&bytes).ok()?;
    match &json[claim] {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_jwt_claims() {
        // {"alg":"none"}.{"xuid":"2535400000000000"}.
        let token = "eyJhbGciOiJub25lIn0.eyJ4dWlkIjoiMjUzNTQwMDAwMDAwMDAwMCJ9.";
        assert_eq!(super::jwt_claim(token, "xuid").as_deref(), Some("2535400000000000"));
    }
}
