//! Minecraft: Bedrock Edition support (Windows only).
#![cfg(target_os = "windows")]

use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PACKAGE_NAME: &str = "Microsoft.MinecraftUWP";
/// Store (UWP) app entry; older installs only.
const UWP_APP_ID: &str = "shell:AppsFolder\\Microsoft.MinecraftUWP_8wekyb3d8bbwe!App";

fn quiet(command: &mut Command) -> bool {
    command
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Bedrock registers the `minecraft:` URL protocol in both its UWP and GDK
/// (Xbox app) packagings, so it's the most reliable signal and launch route.
fn protocol_registered() -> bool {
    quiet(Command::new("reg").args(["query", r"HKCR\minecraft", "/v", "URL Protocol"]))
}

fn package_installed() -> bool {
    quiet(Command::new("powershell").args([
        "-NoProfile",
        "-Command",
        &format!("if (Get-AppxPackage -Name {PACKAGE_NAME}) {{ exit 0 }} else {{ exit 1 }}"),
    ]))
}

/// Checks whether Bedrock Edition is installed.
#[tauri::command]
pub async fn bedrock_is_installed() -> bool {
    tauri::async_runtime::spawn_blocking(|| protocol_registered() || package_installed())
        .await
        .unwrap_or(false)
}

/// Launches Bedrock Edition.
#[tauri::command]
pub fn bedrock_launch() -> Result<(), String> {
    let target = if protocol_registered() { "minecraft:" } else { UWP_APP_ID };
    Command::new("explorer.exe")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
