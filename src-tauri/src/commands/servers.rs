use crate::error::LauncherError;
use crate::servers::{self, LocalServer};
use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn list_local_servers(
    state: State<'_, AppState>,
) -> Result<Vec<LocalServer>, LauncherError> {
    state.cache.list_servers().await
}

#[tauri::command]
pub async fn save_local_server(
    state: State<'_, AppState>,
    mut server: LocalServer,
) -> Result<LocalServer, LauncherError> {
    server.name = server.name.trim().to_string();
    server.game_version = server.game_version.trim().to_string();
    if server.name.is_empty() || server.name.chars().count() > 64 {
        return Err(LauncherError::Message(
            "Server name must be 1-64 characters".into(),
        ));
    }
    if server.game_version.is_empty()
        || server.game_version.len() > 32
        || !server
            .game_version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
    {
        return Err(LauncherError::Message(
            "Minecraft version is invalid".into(),
        ));
    }
    if server.port == 0 {
        return Err(LauncherError::Message("Server port is invalid".into()));
    }
    if !crate::domain::instance_id_is_safe(&server.id) && !server.id.trim().is_empty() {
        return Err(LauncherError::Message(
            "Invalid local server identifier".into(),
        ));
    }
    if let Some(build) = server.build.as_deref() {
        let valid = match server.kind {
            crate::servers::ServerKind::Paper => {
                build.eq_ignore_ascii_case("folia")
                    || build.is_empty()
                    || build.chars().all(|ch| ch.is_ascii_digit())
            }
            crate::servers::ServerKind::Fabric => {
                build.is_empty()
                    || build
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
            }
            crate::servers::ServerKind::Vanilla => build.is_empty(),
        };
        if !valid {
            return Err(LauncherError::Message("Invalid build/project value".into()));
        }
    }
    if server.id.trim().is_empty() {
        server.id = uuid::Uuid::new_v4().to_string();
    }
    if server.created_unix == 0 {
        server.created_unix = unix_now();
    }
    state.cache.upsert_server(server.clone()).await?;
    Ok(server)
}

#[tauri::command]
pub async fn delete_local_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), LauncherError> {
    if !crate::domain::instance_id_is_safe(&id) {
        return Err(LauncherError::Message(
            "Invalid local server identifier".into(),
        ));
    }
    state.servers.stop(&id).await?;
    state.cache.delete_server(&id).await
}

#[tauri::command]
pub async fn install_local_server(
    state: State<'_, AppState>,
    server: LocalServer,
) -> Result<String, LauncherError> {
    if !crate::domain::instance_id_is_safe(&server.id) {
        return Err(LauncherError::Message(
            "Invalid local server identifier".into(),
        ));
    }
    if state.servers.is_running(&server.id) {
        return Err(LauncherError::Message(
            "Stop the server before installing or updating its jar".into(),
        ));
    }
    let stored = state
        .cache
        .list_servers()
        .await?
        .into_iter()
        .find(|item| item.id == server.id)
        .ok_or_else(|| LauncherError::Message("Local server does not exist".into()))?;
    let jar = servers::download_server_jar(&state.http, &state.paths, &stored).await?;
    Ok(jar.display().to_string())
}

#[tauri::command]
pub async fn start_local_server(
    app: AppHandle,
    state: State<'_, AppState>,
    server: LocalServer,
) -> Result<u32, LauncherError> {
    if !crate::domain::instance_id_is_safe(&server.id) {
        return Err(LauncherError::Message(
            "Invalid local server identifier".into(),
        ));
    }
    let stored = state
        .cache
        .list_servers()
        .await?
        .into_iter()
        .find(|item| item.id == server.id)
        .ok_or_else(|| LauncherError::Message("Local server does not exist".into()))?;
    if !stored.eula_accepted {
        return Err(LauncherError::Message(
            "Accept the Minecraft EULA in the server manager before starting".into(),
        ));
    }
    let server = stored;
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let handle = app.clone();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            let _ = handle.emit("local-server-log", line);
        }
    });
    servers::start_server(&state.servers, &state.paths, &server, tx).await
}

#[tauri::command]
pub async fn stop_local_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), LauncherError> {
    if !crate::domain::instance_id_is_safe(&id) {
        return Err(LauncherError::Message(
            "Invalid local server identifier".into(),
        ));
    }
    state.servers.stop(&id).await
}

#[tauri::command]
pub fn local_server_running(state: State<'_, AppState>, id: String) -> bool {
    crate::domain::instance_id_is_safe(&id) && state.servers.is_running(&id)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}
