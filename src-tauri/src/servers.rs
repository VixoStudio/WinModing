use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::error::{io_at, LauncherError};
use crate::net::HttpPool;
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerKind {
    Vanilla,
    Paper,
    Fabric,
}

impl ServerKind {
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "paper" => Self::Paper,
            "fabric" => Self::Fabric,
            _ => Self::Vanilla,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Paper => "paper",
            Self::Fabric => "fabric",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServer {
    pub id: String,
    pub name: String,
    pub kind: ServerKind,
    pub game_version: String,
    pub build: Option<String>,
    pub port: u16,
    pub eula_accepted: bool,
    pub jar_path: Option<String>,
    pub created_unix: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerLogLine {
    pub server_id: String,
    pub stream: String,
    pub line: String,
}

#[derive(Clone, Default)]
pub struct RunningServers {
    inner: Arc<Mutex<HashMap<String, Child>>>,
}

static SERVER_DOWNLOAD_LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    OnceLock::new();

fn server_download_lock(key: String) -> Arc<tokio::sync::Mutex<()>> {
    let locks = SERVER_DOWNLOAD_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().unwrap_or_else(|p| p.into_inner());
    locks
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

impl RunningServers {
    pub fn is_running(&self, id: &str) -> bool {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(child) = guard.get_mut(id) {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    guard.remove(id);
                    false
                }
                Ok(None) => true,
            }
        } else {
            false
        }
    }

    pub async fn stop(&self, id: &str) -> Result<(), LauncherError> {
        let child = self
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id);
        if let Some(mut child) = child {
            if let Some(stdin) = child.stdin.as_mut() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(b"stop\n").await;
                let _ = stdin.flush().await;
            }
            if tokio::time::timeout(std::time::Duration::from_secs(20), child.wait())
                .await
                .is_err()
            {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }
        Ok(())
    }

    fn insert(&self, id: String, child: Child) {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, child);
    }
}

pub async fn download_server_jar(
    http: &HttpPool,
    paths: &AppPaths,
    server: &LocalServer,
) -> Result<PathBuf, LauncherError> {
    if !crate::domain::instance_id_is_safe(&server.id) {
        return Err(LauncherError::Message(
            "Invalid local server identifier".into(),
        ));
    }
    if server.game_version.is_empty()
        || !server
            .game_version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
    {
        return Err(LauncherError::Message("Invalid Minecraft version".into()));
    }
    let lock = server_download_lock(format!("{}:{}", paths.root.display(), server.id));
    let _guard = lock.lock().await;
    let dir = paths.root.join("servers").join(&server.id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| io_at(&dir, e))?;
    let jar = dir.join("server.jar");
    let (url, expected_hash) = match server.kind {
        ServerKind::Vanilla => {
            let manifest: Value = http
                .get_json(
                    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
                    &[],
                )
                .await?;
            let version = manifest["versions"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["id"] == server.game_version))
                .ok_or_else(|| {
                    LauncherError::Message(format!(
                        "Minecraft version {} was not found in the official manifest",
                        server.game_version
                    ))
                })?;
            let metadata_url =
                version["url"]
                    .as_str()
                    .ok_or_else(|| LauncherError::CorruptVersion {
                        reason: "version metadata URL missing".into(),
                    })?;
            let metadata: Value = http.get_json(metadata_url, &[]).await?;
            let download = metadata.pointer("/downloads/server").ok_or_else(|| {
                LauncherError::Message("Vanilla server jar is unavailable for this version".into())
            })?;
            (
                download["url"]
                    .as_str()
                    .ok_or_else(|| LauncherError::CorruptVersion {
                        reason: "server jar URL missing".into(),
                    })?
                    .to_string(),
                download["sha1"]
                    .as_str()
                    .map(crate::domain::ContentHash::sha1),
            )
        }
        ServerKind::Paper => {
            let (project, requested_build) = match server.build.as_deref() {
                Some("folia") | Some("Folia") | Some("FOLIA") => ("folia", None),
                Some(value) if !value.is_empty() => ("paper", value.parse::<u64>().ok()),
                _ => ("paper", None),
            };
            let builds: Value = http
                .get_json(
                    &format!(
                        "https://fill.papermc.io/v3/projects/{project}/versions/{}/builds",
                        server.game_version
                    ),
                    &[],
                )
                .await?;
            let build = requested_build
                .or_else(|| {
                    builds
                        .as_array()?
                        .iter()
                        .filter(|b| b["channel"] == "STABLE")
                        .filter_map(|b| b["id"].as_u64())
                        .max()
                })
                .ok_or_else(|| LauncherError::Message("No stable Paper build was found".into()))?;
            let download = builds
                .as_array()
                .and_then(|items| items.iter().find(|b| b["id"].as_u64() == Some(build)))
                .and_then(|b| b.pointer("/downloads/server"))
                .ok_or_else(|| {
                    LauncherError::Message(format!("Paper build {build} has no server download"))
                })?;
            (
                download["url"]
                    .as_str()
                    .ok_or_else(|| LauncherError::CorruptVersion {
                        reason: "Paper server URL missing".into(),
                    })?
                    .to_string(),
                download["checksums"]["sha256"]
                    .as_str()
                    .map(crate::domain::ContentHash::sha256),
            )
        }
        ServerKind::Fabric => {
            let loader = server
                .build
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("0.16.10");
            let url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}/{}/1.0.1/server/jar",
                server.game_version, loader
            );
            (url, None)
        }
    };
    if !url.starts_with("https://") {
        return Err(LauncherError::Message(
            "Refusing non-HTTPS server jar URL".into(),
        ));
    }
    match expected_hash {
        Some(hash) => http.download_verified(&url, &jar, &hash).await?,
        None => http.download_to(&url, &jar).await?,
    }
    Ok(jar)
}

pub async fn start_server(
    running: &RunningServers,
    paths: &AppPaths,
    server: &LocalServer,
    log_tx: mpsc::Sender<ServerLogLine>,
) -> Result<u32, LauncherError> {
    if !crate::domain::instance_id_is_safe(&server.id) {
        return Err(LauncherError::Message(
            "Invalid local server identifier".into(),
        ));
    }
    if running.is_running(&server.id) {
        return Err(LauncherError::AlreadyRunning {
            id: server.id.clone(),
        });
    }
    if !server.eula_accepted {
        return Err(LauncherError::Message(
            "You must accept the Minecraft EULA before starting this server".into(),
        ));
    }
    let dir = paths.root.join("servers").join(&server.id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| io_at(&dir, e))?;
    let jar = server
        .jar_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("server.jar"));
    if !jar.is_file() {
        return Err(LauncherError::MissingAssets {
            detail: format!("server jar not found: {}", jar.display()),
        });
    }
    tokio::fs::write(dir.join("eula.txt"), "eula=true\n")
        .await
        .map_err(|e| io_at(dir.join("eula.txt"), e))?;
    let mut command = Command::new("java");
    command
        .current_dir(&dir)
        .args(["-jar", jar.to_string_lossy().as_ref(), "nogui"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|e| io_at("java", e))?;
    let pid = child.id().unwrap_or(0);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    pump(&server.id, "stdout", stdout, log_tx.clone());
    pump(&server.id, "stderr", stderr, log_tx);
    running.insert(server.id.clone(), child);
    Ok(pid)
}

fn pump<R>(id: &str, stream: &str, pipe: Option<R>, tx: mpsc::Sender<ServerLogLine>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let Some(pipe) = pipe else { return };
    let id = id.to_string();
    let stream = stream.to_string();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut lines = BufReader::new(pipe).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx
                .send(ServerLogLine {
                    server_id: id.clone(),
                    stream: stream.clone(),
                    line,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });
}
