use crate::domain::LoaderKind;
use crate::error::LauncherError;
use crate::launch::args::{merge_documents, parse_version_document, VersionDocument};
use crate::net::HttpPool;
use crate::paths::AppPaths;
use crate::protocol::{
    FABRIC_GAME_META, FABRIC_GAME_META_MIRRORS, FABRIC_LOADER_META, FABRIC_LOADER_META_MIRRORS,
    QUILT_LOADER_META, QUILT_LOADER_META_MIRRORS,
};

#[derive(Debug, Clone, serde::Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderMeta,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct FabricLoaderMeta {
    version: String,
    #[serde(default)]
    stable: bool,
}

pub async fn resolve_document(
    http: &HttpPool,
    paths: &AppPaths,
    game_version: &str,
    loader: LoaderKind,
    loader_version: Option<&str>,
) -> Result<VersionDocument, LauncherError> {
    let vanilla_path = paths.version_json(game_version);
    let vanilla_text = tokio::fs::read_to_string(&vanilla_path)
        .await
        .map_err(|_| LauncherError::MissingAssets {
            detail: format!("version {game_version} is not installed"),
        })?;
    let vanilla = parse_version_document(&vanilla_text)?;
    match loader {
        LoaderKind::Vanilla => Ok(vanilla),
        LoaderKind::Fabric | LoaderKind::Quilt => {
            let child =
                ensure_fabric_like(http, paths, game_version, loader, loader_version).await?;
            Ok(merge_documents(vanilla, child))
        }
        LoaderKind::Pomme | LoaderKind::Horizon | LoaderKind::Win => Err(LauncherError::Message(
            "Native clients are not Java launch profiles".into(),
        )),
        LoaderKind::Forge | LoaderKind::NeoForge => Err(LauncherError::Message(format!(
            "{} is not installed yet — create a Fabric or Vanilla profile",
            loader.as_str()
        ))),
    }
}

async fn ensure_fabric_like(
    http: &HttpPool,
    paths: &AppPaths,
    game_version: &str,
    loader: LoaderKind,
    requested: Option<&str>,
) -> Result<VersionDocument, LauncherError> {
    let version = resolve_loader_version(http, game_version, loader, requested).await?;
    let json_path = paths.loader_json(loader.as_str(), game_version, &version);
    if json_path.exists() {
        let text = tokio::fs::read_to_string(&json_path)
            .await
            .map_err(|err| crate::error::io_at(&json_path, err))?;
        return parse_version_document(&text);
    }
    let meta_urls = match loader {
        LoaderKind::Quilt => QUILT_LOADER_META_MIRRORS,
        LoaderKind::Pomme | LoaderKind::Horizon | LoaderKind::Win => {
            return Err(LauncherError::Message(
                "Native clients are not Java launch profiles".into(),
            ))
        }
        _ => FABRIC_LOADER_META_MIRRORS,
    };
    let mut last_err = None;
    for meta in meta_urls {
        let url = format!("{meta}/{game_version}/{version}/profile/json");
        if crate::protocol::assert_trusted_https(&url).is_err() {
            continue;
        }
        match http.get_text(&url, &[]).await {
            Ok(text) => {
                if text.len() < 32 {
                    last_err = Some(LauncherError::CorruptVersion {
                        reason: format!("loader profile from {url} was too small"),
                    });
                    continue;
                }
                log::warn!(
                    "loader profile {url} has no content hash; HTTPS host and size were checked"
                );
                if let Some(parent) = json_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|err| crate::error::io_at(parent, err))?;
                }
                tokio::fs::write(&json_path, &text)
                    .await
                    .map_err(|err| crate::error::io_at(&json_path, err))?;
                return parse_version_document(&text);
            }
            Err(err) => {
                log::warn!("loader profile mirror {} failed: {}", url, err);
                last_err = Some(err);
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| LauncherError::Message("All loader profile mirrors failed".into())))
}

pub async fn resolve_loader_version(
    http: &HttpPool,
    game_version: &str,
    loader: LoaderKind,
    requested: Option<&str>,
) -> Result<String, LauncherError> {
    let wanted = requested.map(str::trim).filter(|value| {
        !value.is_empty() && *value != "stable" && *value != "latest" && *value != "other"
    });
    if let Some(version) = wanted {
        return Ok(version.to_string());
    }
    let meta_urls = match loader {
        LoaderKind::Quilt => QUILT_LOADER_META_MIRRORS,
        LoaderKind::Pomme | LoaderKind::Horizon | LoaderKind::Win => {
            return Err(LauncherError::Message(
                "Native clients are not Java launch profiles".into(),
            ))
        }
        _ => FABRIC_LOADER_META_MIRRORS,
    };
    let mut last_err = None;
    for meta in meta_urls {
        let url = format!("{meta}/{game_version}");
        match http.get_json::<Vec<FabricLoaderEntry>>(&url, &[]).await {
            Ok(entries) => {
                let prefer_stable = requested != Some("latest");
                if prefer_stable {
                    if let Some(entry) = entries.iter().find(|entry| entry.loader.stable) {
                        return Ok(entry.loader.version.clone());
                    }
                }
                return entries
                    .into_iter()
                    .next()
                    .map(|entry| entry.loader.version)
                    .ok_or_else(|| {
                        LauncherError::Message(format!(
                            "no {} loader for Minecraft {game_version}",
                            loader.as_str()
                        ))
                    });
            }
            Err(err) => {
                log::warn!("Loader metadata mirror {} failed: {}", url, err);
                last_err = Some(err);
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| LauncherError::Message("All loader metadata mirrors failed".into())))
}
