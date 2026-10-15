// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod minecraft_java;
mod minecraft_java_install;
mod microsoft_auth;

#[cfg(target_os = "windows")]
mod minecraft_bedrock;

/// Lets the frontend decide whether to show Bedrock UI.
#[tauri::command]
fn bedrock_supported() -> bool {
    cfg!(target_os = "windows")
}

#[cfg(target_os = "windows")]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        bedrock_supported,
        minecraft_java::java_game_dir,
        minecraft_java::java_list_versions,
        minecraft_java::java_launch,
        minecraft_java_install::java_available_versions,
        minecraft_java_install::java_prepare,
        minecraft_java_install::java_cancel_prepare,
        minecraft_java::system_memory_mb,
        microsoft_auth::msa_status,
        microsoft_auth::msa_configured,
        microsoft_auth::msa_begin_login,
        microsoft_auth::msa_open_verification,
        microsoft_auth::msa_finish_login,
        microsoft_auth::msa_cancel_login,
        microsoft_auth::msa_logout,
        minecraft_bedrock::bedrock_is_installed,
        minecraft_bedrock::bedrock_launch,
    ]
}

#[cfg(not(target_os = "windows"))]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        bedrock_supported,
        minecraft_java::java_game_dir,
        minecraft_java::java_list_versions,
        minecraft_java::java_launch,
        minecraft_java_install::java_available_versions,
        minecraft_java_install::java_prepare,
        minecraft_java_install::java_cancel_prepare,
        minecraft_java::system_memory_mb,
        microsoft_auth::msa_status,
        microsoft_auth::msa_configured,
        microsoft_auth::msa_begin_login,
        microsoft_auth::msa_open_verification,
        microsoft_auth::msa_finish_login,
        microsoft_auth::msa_cancel_login,
        microsoft_auth::msa_logout,
    ]
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(handlers())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
