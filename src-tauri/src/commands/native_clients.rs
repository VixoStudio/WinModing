use crate::error::LauncherError;
use crate::native_clients::{self, NativeClientRelease};
use crate::state::AppState;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::State;

#[tauri::command]
pub async fn save_native_client_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    client: String,
    server: Option<String>,
) -> Result<crate::domain::InstanceRecord, LauncherError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(LauncherError::Message(
            "Profile name must contain between 1 and 64 characters".into(),
        ));
    }
    let client_name = client.trim().to_ascii_lowercase();
    if client_name != "win" {
        return Err(LauncherError::Message("Unknown native client".into()));
    }
    let client = crate::domain::LoaderKind::Win;
    let release = native_clients::latest(&state.http, &state.paths, &client_name).await?;
    if let Some(installed_path) = release.installed_path.as_deref() {
        if !native_clients::is_playable_client(std::path::Path::new(installed_path)) {
            let _ = tokio::fs::remove_file(installed_path).await;
        }
    }
    let installed_path = native_clients::install(&state.http, &state.paths, &client_name).await?;
    if !std::path::Path::new(&installed_path).is_file() {
        return Err(LauncherError::Message(
            "Rust client installation did not produce an executable".into(),
        ));
    }
    if !native_clients::is_playable_client(std::path::Path::new(&installed_path)) {
        let _ = tokio::fs::remove_file(&installed_path).await;
        return Err(LauncherError::Message(
            "The published Rust download is not a playable Minecraft client. A real Rust game client must be published before this profile can be created.".into(),
        ));
    }
    let installed = native_clients::latest(&state.http, &state.paths, &client_name).await?;
    let id = crate::commands::slug_id(name);
    if !crate::domain::instance_id_is_safe(&id) {
        return Err(LauncherError::Message("Invalid profile name".into()));
    }
    let record = crate::domain::InstanceRecord {
        id,
        name: name.to_string(),
        game_version: installed.tag,
        loader: client,
        loader_version: None,
        java_major: None,
        created_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        sort_order: 0,
    };
    let dir = state.paths.instance_dir(&record.id);
    std::fs::create_dir_all(&dir).map_err(|error| crate::error::io_at(&dir, error))?;
    crate::content::assign_random_icon(&dir)?;
    if let Some(address) = server
        .as_deref()
        .map(str::trim)
        .filter(|address| !address.is_empty())
    {
        if address.len() > 255 || address.chars().any(char::is_whitespace) {
            return Err(LauncherError::Message("Invalid server address".into()));
        }
        let mut options = crate::content::read_options(&dir);
        options["quickServers"] = serde_json::json!([{ "name": address, "address": address }]);
        crate::content::write_options(&dir, &options)?;
    }
    state.cache.insert_instance(record.clone()).await?;
    let mut extra = crate::content::read_options(&dir);
    crate::content::attach_icon_data(&dir, &mut extra);
    let icon_data_url = extra
        .get("iconDataUrl")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let icon_path = extra
        .get("iconPath")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let mut event = serde_json::json!({
        "uuid": record.id,
        "name": record.name,
        "profile_path": record.id,
        "path": record.id,
        "event": "Created",
        "nativeClient": "win"
    });
    if let Some(object) = event.as_object_mut() {
        if let Some(icon) = icon_data_url {
            object.insert("iconDataUrl".into(), serde_json::Value::String(icon));
        }
        if let Some(path) = icon_path {
            object.insert("iconPath".into(), serde_json::Value::String(path));
        }
    }
    let _ = app.emit("profile", event);
    Ok(record)
}

#[tauri::command]
pub async fn native_client_latest(
    state: State<'_, AppState>,
    client: String,
) -> Result<NativeClientRelease, LauncherError> {
    native_clients::latest(&state.http, &state.paths, &client).await
}

#[tauri::command]
pub async fn launch_native_client(
    state: State<'_, AppState>,
    client: String,
    server: Option<String>,
) -> Result<u32, LauncherError> {
    let release = native_clients::latest(&state.http, &state.paths, &client).await?;
    let path = release.installed_path.ok_or_else(|| {
        LauncherError::Message(
            "A playable Rust Minecraft client is not installed yet. Create the Rust profile first."
                .into(),
        )
    })?;
    let expected_sha256 = release
        .binary_sha256
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "Rust release manifest did not provide an executable checksum".into(),
        })?;
    native_clients::verify_installed_client_for_launch(
        std::path::Path::new(&path),
        &expected_sha256,
    )?;
    let mut command = tokio::process::Command::new(&path);
    if let Some(parent) = std::path::Path::new(&path).parent() {
        command.current_dir(parent);
    }
    if let Some(address) = server
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if address.len() > 255 || address.chars().any(char::is_whitespace) {
            return Err(LauncherError::Message("Invalid server address".into()));
        }
        command.arg(address);
    }
    let child = command.spawn().map_err(|error| {
        LauncherError::Message(format!("Could not launch native client: {error}"))
    })?;
    Ok(child.id().unwrap_or_default())
}

#[tauri::command]
pub fn native_client_kill(pid: u32) -> Result<(), LauncherError> {
    use sysinfo::System;
    let mut system = System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
        process.kill();
    }
    Ok(())
}

#[tauri::command]
pub fn native_client_exited(pid: u32) -> bool {
    use sysinfo::System;
    let mut system = System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    !system.process(sysinfo::Pid::from_u32(pid)).is_some()
}

#[tauri::command]
pub async fn install_native_client(
    state: State<'_, AppState>,
    client: String,
) -> Result<String, LauncherError> {
    native_clients::install(&state.http, &state.paths, &client).await
}
