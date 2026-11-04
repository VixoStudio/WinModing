use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use serde_json::Value;

use crate::error::{io_at, LauncherError};
use crate::net::HttpPool;
use crate::paths::AppPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeClient {
    Pomme,
    Horizon,
    Win,
}

impl NativeClient {
    fn repo(self) -> &'static str {
        WIN_CLIENT_REPO
    }
    fn asset_name(self) -> &'static str {
        WINDOWS_ASSET
    }
    fn executable_name(self) -> &'static str {
        "win-client.exe"
    }

    fn parse(value: &str) -> Result<Self, LauncherError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "win" | "pomme" | "horizon" => Ok(Self::Win),
            _ => Err(LauncherError::Message("Unknown native client".into())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeClientRelease {
    pub client: NativeClient,
    pub tag: String,
    pub release_url: String,
    pub installed_path: Option<String>,
    pub binary_sha256: Option<String>,
}

const WIN_CLIENT_REPO: &str = "VixoStudio/Win-Client";
const WINDOWS_ASSET: &str = "win-client-windows-x64.zip";

#[derive(Debug, Clone, Deserialize)]
struct ReleaseAsset {
    name: String,
    sha256: String,
    url: Option<String>,
}

#[derive(Debug, Clone)]
struct ReleaseContract {
    release_tag: String,
    source_asset: ReleaseAsset,
    assets: Vec<ReleaseAsset>,
    source_commit: String,
    steelmc_commit: String,
    executable_sha256: String,
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_release_manifest(value: &Value) -> Result<ReleaseContract, LauncherError> {
    let release_tag = value["release_tag"]
        .as_str()
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest release_tag missing".into(),
        })?;
    let source_name = value["source_asset"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest source_asset missing".into(),
        })?;
    let source_sha = value["source_sha256"]
        .as_str()
        .filter(|hash| valid_sha256(hash))
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest source_sha256 missing or malformed".into(),
        })?;
    let assets = value["assets"]
        .as_array()
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest assets missing".into(),
        })?;
    let assets = assets
        .iter()
        .map(|asset| {
            let name = asset["name"]
                .as_str()
                .filter(|name| {
                    !name.is_empty()
                        && Path::new(name).file_name().and_then(|value| value.to_str())
                            == Some(*name)
                })
                .ok_or_else(|| LauncherError::CorruptVersion {
                    reason: "manifest asset name missing".into(),
                })?;
            let sha256 = asset["sha256"]
                .as_str()
                .filter(|hash| valid_sha256(hash))
                .ok_or_else(|| LauncherError::CorruptVersion {
                    reason: format!("manifest checksum missing or malformed for {name}"),
                })?;
            Ok(ReleaseAsset {
                name: name.to_string(),
                sha256: sha256.to_ascii_lowercase(),
                url: asset["browser_download_url"].as_str().map(str::to_string),
            })
        })
        .collect::<Result<Vec<_>, LauncherError>>()?;
    let mut names = assets
        .iter()
        .map(|asset| asset.name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(LauncherError::CorruptVersion {
            reason: "manifest contains duplicate asset names".into(),
        });
    }
    let source_asset = assets
        .iter()
        .find(|asset| asset.name == source_name)
        .cloned()
        .filter(|asset| asset.sha256 == source_sha.to_ascii_lowercase())
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest source asset/checksum mismatch".into(),
        })?;
    let source_commit = value["source_commit"]
        .as_str()
        .filter(|commit| !commit.is_empty())
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest source_commit missing".into(),
        })?;
    let steelmc_commit = value["steelmc_commit"]
        .as_str()
        .filter(|commit| !commit.is_empty())
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest steelmc_commit missing".into(),
        })?;
    let executable_sha256 = value["executable_sha256"]
        .as_str()
        .filter(|hash| valid_sha256(hash))
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "manifest executable_sha256 missing or malformed".into(),
        })?;
    Ok(ReleaseContract {
        release_tag: release_tag.to_string(),
        source_asset,
        assets,
        source_commit: source_commit.to_string(),
        steelmc_commit: steelmc_commit.to_string(),
        executable_sha256: executable_sha256.to_ascii_lowercase(),
    })
}

fn select_windows_asset(contract: &ReleaseContract) -> Result<ReleaseAsset, LauncherError> {
    contract
        .assets
        .iter()
        .find(|asset| asset.name == WINDOWS_ASSET)
        .cloned()
        .ok_or_else(|| {
            LauncherError::Message(format!(
                "Windows asset {WINDOWS_ASSET} is missing from release manifest"
            ))
        })
}

fn validate_release_asset_set(
    release_assets: &[String],
    contract: &ReleaseContract,
    tag: &str,
) -> Result<(), LauncherError> {
    let mut actual = release_assets.to_vec();
    actual.sort_unstable();
    actual.dedup();
    if actual.len() != release_assets.len() {
        return Err(LauncherError::CorruptVersion {
            reason: "GitHub release contains duplicate asset names".into(),
        });
    }
    let mut expected: Vec<String> = contract
        .assets
        .iter()
        .map(|asset| asset.name.clone())
        .collect();
    expected.sort_unstable();
    expected.dedup();
    let manifest_name = format!("Win-Client-{tag}-manifest.json");
    if expected.len() != contract.assets.len()
        || actual != expected
        || actual
            .iter()
            .any(|name| name == &manifest_name || name == "sha256sums.txt")
    {
        return Err(LauncherError::CorruptVersion {
            reason: "manifest assets do not exactly match GitHub release assets".into(),
        });
    }
    Ok(())
}

fn validate_source_archive(
    archive_path: &Path,
    expected_steelmc_commit: &str,
    expected_source_commit: &str,
) -> Result<(), LauncherError> {
    let file = std::fs::File::open(archive_path).map_err(|error| io_at(archive_path, error))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut steel_marker = None;
    let mut steel_metadata_marker = None;
    let mut source_metadata_marker = None;
    let mut has_steel_source = false;
    for entry in archive
        .entries()
        .map_err(|error| LauncherError::CorruptVersion {
            reason: format!("corresponding source archive cannot be read: {error}"),
        })?
    {
        let mut entry = entry.map_err(|error| LauncherError::CorruptVersion {
            reason: format!("corresponding source archive entry is invalid: {error}"),
        })?;
        let path = entry
            .path()
            .map_err(|error| LauncherError::CorruptVersion {
                reason: format!("corresponding source archive path is invalid: {error}"),
            })?
            .to_string_lossy()
            .replace('\\', "/");
        if path.ends_with("third_party/SteelMC/.git-submodule-commit") {
            let mut value = String::new();
            std::io::Read::read_to_string(&mut entry, &mut value).map_err(|error| {
                LauncherError::CorruptVersion {
                    reason: format!("SteelMC commit marker cannot be read: {error}"),
                }
            })?;
            steel_marker = Some(value.trim().to_string());
        } else if path.ends_with("third_party/SteelMC/.git-commit-metadata") {
            let mut value = String::new();
            std::io::Read::read_to_string(&mut entry, &mut value).map_err(|error| {
                LauncherError::CorruptVersion {
                    reason: format!("SteelMC metadata cannot be read: {error}"),
                }
            })?;
            if value.lines().next().map(str::trim) == Some(expected_steelmc_commit) {
                steel_metadata_marker = Some(value);
            }
        } else if path.contains("third_party/SteelMC/")
            && !path.ends_with("third_party/SteelMC/")
            && !path.ends_with("third_party/SteelMC/.git-submodule-commit")
            && !path.ends_with("third_party/SteelMC/.git-commit-metadata")
        {
            has_steel_source = true;
        } else if path.ends_with("/.git-commit-metadata") || path == ".git-commit-metadata" {
            let mut value = String::new();
            std::io::Read::read_to_string(&mut entry, &mut value).map_err(|error| {
                LauncherError::CorruptVersion {
                    reason: format!("source commit metadata cannot be read: {error}"),
                }
            })?;
            if value.lines().next().map(str::trim) == Some(expected_source_commit) {
                source_metadata_marker = Some(value);
            }
        }
    }
    if steel_marker.as_deref() != Some(expected_steelmc_commit)
        || !has_steel_source
        || steel_metadata_marker.is_none()
        || source_metadata_marker.is_none()
    {
        return Err(LauncherError::CorruptVersion {
            reason:
                "corresponding source archive is missing pinned SteelMC source or commit metadata"
                    .into(),
        });
    }
    Ok(())
}

pub fn verify_installed_client_for_launch(
    path: &Path,
    expected_sha256: &str,
) -> Result<(), LauncherError> {
    if !cached_valid(path, expected_sha256) {
        return Err(LauncherError::HashMismatch {
            path: path.display().to_string(),
            expected: expected_sha256.to_string(),
            actual: "installed executable mismatch".into(),
        });
    }
    if !is_playable_client(path) {
        return Err(LauncherError::Message(
            "The installed Win file is not a playable Minecraft client".into(),
        ));
    }
    Ok(())
}

fn atomic_replace(source: &Path, destination: &Path) -> Result<(), LauncherError> {
    let backup = PathBuf::from(format!("{}.backup", destination.display()));
    let _ = std::fs::remove_file(&backup);
    let had_destination = destination.exists();
    if had_destination {
        std::fs::rename(destination, &backup).map_err(|error| io_at(destination, error))?;
    }
    match std::fs::rename(source, destination) {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Err(error) => {
            if had_destination {
                let _ = std::fs::rename(&backup, destination);
            }
            let _ = std::fs::remove_file(source);
            Err(io_at(destination, error))
        }
    }
}

fn extract_verified_executable(
    archive_path: &Path,
    destination: &Path,
    expected_sha256: &str,
) -> Result<(), LauncherError> {
    let file = std::fs::File::open(archive_path).map_err(|error| io_at(archive_path, error))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| LauncherError::CorruptVersion {
            reason: format!("Windows client ZIP is invalid: {error}"),
        })?;
    let mut executable =
        archive
            .by_name("win-client.exe")
            .map_err(|error| LauncherError::CorruptVersion {
                reason: format!("Windows client ZIP is missing win-client.exe: {error}"),
            })?;
    let partial = PathBuf::from(format!("{}.partial", destination.display()));
    let result = (|| {
        let mut output = std::fs::File::create(&partial).map_err(|error| io_at(&partial, error))?;
        std::io::copy(&mut executable, &mut output).map_err(|error| io_at(&partial, error))?;
        output.sync_all().map_err(|error| io_at(&partial, error))?;
        if !is_playable_client(&partial) {
            return Err(LauncherError::CorruptVersion {
                reason: "win-client.exe in the archive is not a playable Windows executable".into(),
            });
        }
        if !cached_valid(&partial, expected_sha256) {
            return Err(LauncherError::HashMismatch {
                path: partial.display().to_string(),
                expected: expected_sha256.to_string(),
                actual: "extracted executable hash mismatch".into(),
            });
        }
        atomic_replace(&partial, destination)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

fn cached_valid(path: &Path, expected_sha256: &str) -> bool {
    path.is_file()
        && crate::hashutil::verify_file(path, &crate::domain::ContentHash::sha256(expected_sha256))
            .is_ok()
}

fn install_verified_file(
    source: &Path,
    binary: &Path,
    destination: &Path,
    source_sha256: &str,
    binary_sha256: &str,
) -> Result<(), LauncherError> {
    if !cached_valid(source, source_sha256) {
        return Err(LauncherError::HashMismatch {
            path: source.display().to_string(),
            expected: source_sha256.to_string(),
            actual: "source mismatch".into(),
        });
    }
    if !cached_valid(binary, binary_sha256) {
        return Err(LauncherError::HashMismatch {
            path: binary.display().to_string(),
            expected: binary_sha256.to_string(),
            actual: "binary mismatch".into(),
        });
    }
    let partial = PathBuf::from(format!("{}.partial", destination.display()));
    std::fs::copy(binary, &partial).map_err(|error| io_at(&partial, error))?;
    if let Err(error) = std::fs::rename(&partial, destination) {
        let _ = std::fs::remove_file(&partial);
        return Err(io_at(destination, error));
    }
    Ok(())
}

pub fn is_playable_client(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Some(pe) = bytes
        .get(0x3c..0x40)
        .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
        .map(u32::from_le_bytes)
        .map(|offset| offset as usize)
    else {
        return false;
    };
    let is_pe = bytes.starts_with(b"MZ") && bytes.get(pe..pe.saturating_add(4)) == Some(b"PE\0\0");
    let identifies_as_launcher = bytes
        .get(..bytes.len().min(1024 * 1024))
        .and_then(|slice| std::str::from_utf8(slice).ok())
        .is_some_and(|text| text.contains("WinModing"));
    is_pe && !identifies_as_launcher
}

static NATIVE_INSTALLS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    OnceLock::new();

fn install_lock(key: String) -> Arc<tokio::sync::Mutex<()>> {
    let locks = NATIVE_INSTALLS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().unwrap_or_else(|p| p.into_inner());
    locks
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

async fn fetch_release(http: &HttpPool, client: NativeClient) -> Result<Value, LauncherError> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        client.repo()
    );
    let release: Value = http
        .get_json(&url, &[("Accept", "application/vnd.github+json")])
        .await?;
    let html_url = release["html_url"].as_str().unwrap_or_default();
    if !html_url.starts_with(&format!(
        "https://github.com/{}/releases/tag/",
        client.repo()
    )) {
        return Err(LauncherError::Message(
            "GitHub returned an unexpected release URL".into(),
        ));
    }
    Ok(release)
}

async fn fetch_contract(
    http: &HttpPool,
    client: NativeClient,
) -> Result<(Value, String, ReleaseContract, ReleaseAsset), LauncherError> {
    let release = fetch_release(http, client).await?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "release tag missing".into(),
        })?
        .to_string();
    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "release assets missing".into(),
        })?;
    let find_asset = |name: &str| {
        assets
            .iter()
            .find(|item| item["name"].as_str() == Some(name))
    };
    let manifest_name = format!("Win-Client-{tag}-manifest.json");
    let manifest_url = find_asset(&manifest_name)
        .and_then(|item| item["browser_download_url"].as_str())
        .ok_or_else(|| LauncherError::Message("Win release manifest is missing".into()))?;
    let manifest: Value = http
        .get_json(manifest_url, &[("Accept", "application/json")])
        .await?;
    let contract = parse_release_manifest(&manifest)?;
    if contract.release_tag != tag {
        return Err(LauncherError::CorruptVersion {
            reason: "manifest release tag does not match release".into(),
        });
    }
    let release_asset_names = assets
        .iter()
        .filter_map(|item| item["name"].as_str())
        .filter(|name| *name != manifest_name && *name != "sha256sums.txt")
        .map(str::to_string)
        .collect::<Vec<_>>();
    validate_release_asset_set(&release_asset_names, &contract, &tag)?;
    let binary_asset = select_windows_asset(&contract)?;
    Ok((release, tag, contract, binary_asset))
}

pub async fn validated_binary_sha256(
    http: &HttpPool,
    client_name: &str,
) -> Result<String, LauncherError> {
    let client = NativeClient::parse(client_name)?;
    let (_, _, contract, _) = fetch_contract(http, client).await?;
    Ok(contract.executable_sha256)
}

pub async fn latest(
    http: &HttpPool,
    paths: &AppPaths,
    client_name: &str,
) -> Result<NativeClientRelease, LauncherError> {
    let client = NativeClient::parse(client_name)?;
    let (release, tag, contract, _binary_asset) = fetch_contract(http, client).await?;
    let release_url = release["html_url"].as_str().unwrap_or_default().to_string();
    let executable = paths
        .native_clients_dir()
        .join("win")
        .join(client.executable_name());
    let installed_path = executable
        .is_file()
        .then(|| executable.display().to_string());
    let binary_sha256 = Some(contract.executable_sha256.clone());
    Ok(NativeClientRelease {
        client,
        tag,
        release_url,
        installed_path,
        binary_sha256,
    })
}

pub async fn install(
    http: &HttpPool,
    paths: &AppPaths,
    client_name: &str,
) -> Result<String, LauncherError> {
    let client = NativeClient::parse(client_name)?;
    let base = paths.native_clients_dir().join("win");
    let lock = install_lock(base.display().to_string());
    let _guard = lock.lock().await;
    let (release, _tag, contract, binary_asset) = fetch_contract(http, client).await?;
    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| LauncherError::CorruptVersion {
            reason: "release assets missing".into(),
        })?;
    let find_asset = |name: &str| {
        assets
            .iter()
            .find(|item| item["name"].as_str() == Some(name))
    };
    let asset_url = find_asset(&binary_asset.name)
        .and_then(|item| item["browser_download_url"].as_str())
        .ok_or_else(|| {
            LauncherError::Message("Win Windows asset URL is missing from release".into())
        })?
        .to_string();
    let source_url = find_asset(&contract.source_asset.name)
        .and_then(|item| item["browser_download_url"].as_str())
        .ok_or_else(|| {
            LauncherError::Message("Win corresponding source asset is missing from release".into())
        })?
        .to_string();
    tokio::fs::create_dir_all(&base)
        .await
        .map_err(|e| io_at(&base, e))?;
    let binary_partial = base.join(format!("{}.partial", client.asset_name()));
    let source_path = base.join(&contract.source_asset.name);
    if !cached_valid(&source_path, &contract.source_asset.sha256) {
        http.download_verified(
            &source_url,
            &source_path,
            &crate::domain::ContentHash::sha256(&contract.source_asset.sha256),
        )
        .await?;
    }
    let partial = binary_partial;
    if !cached_valid(&partial, &binary_asset.sha256) {
        http.download_verified(
            &asset_url,
            &partial,
            &crate::domain::ContentHash::sha256(&binary_asset.sha256),
        )
        .await?;
    }
    validate_source_archive(
        &source_path,
        &contract.steelmc_commit,
        &contract.source_commit,
    )?;
    crate::hashutil::verify_file(
        &partial,
        &crate::domain::ContentHash::sha256(&binary_asset.sha256),
    )?;
    let executable = base.join(client.executable_name());
    extract_verified_executable(&partial, &executable, &contract.executable_sha256)?;
    Ok(executable.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_and_selects_windows_zip_and_source() {
        let manifest = serde_json::json!({
            "release_tag": "client-v1.2.3",
            "source_asset": "Win-Client-client-v1.2.3-source.tar.gz",
            "source_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "source_commit": "source-commit",
            "steelmc_commit": "steel-commit",
            "executable_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "assets": [
                {"name": "win-client-windows-x64.zip", "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
                {"name": "Win-Client-client-v1.2.3-source.tar.gz", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
            ]
        });
        let contract = parse_release_manifest(&manifest).unwrap();
        assert_eq!(contract.release_tag, "client-v1.2.3");
        assert_eq!(
            select_windows_asset(&contract).unwrap().name,
            "win-client-windows-x64.zip"
        );
        assert_eq!(
            contract.source_asset.name,
            "Win-Client-client-v1.2.3-source.tar.gz"
        );
    }

    #[test]
    fn rejects_missing_or_malformed_manifest_metadata() {
        for manifest in [
            serde_json::json!({}),
            serde_json::json!({"release_tag":"v1","assets":[]}),
        ] {
            assert!(parse_release_manifest(&manifest).is_err());
        }
    }

    #[test]
    fn rejects_wrong_windows_asset() {
        let manifest = serde_json::json!({
            "release_tag":"v1", "source_asset":"source.tar.gz",
            "source_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "source_commit":"source-commit",
            "steelmc_commit":"steel-commit",
            "executable_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "assets":[
                {"name":"source.tar.gz","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                {"name":"win-client-windows-arm64.zip","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
            ]
        });
        let contract = parse_release_manifest(&manifest).unwrap();
        assert!(select_windows_asset(&contract).is_err());
    }

    #[test]
    fn verifies_source_and_binary_and_cleans_partial() {
        let dir = std::env::temp_dir().join(format!("winmoding-native-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.tar.gz");
        let binary = dir.join("binary.zip");
        let destination = dir.join("installed.zip");
        std::fs::write(&source, b"source").unwrap();
        std::fs::write(&binary, b"binary").unwrap();
        let source_hash = crate::hashutil::digest_bytes(b"source", crate::domain::HashAlgo::Sha256);
        let binary_hash = crate::hashutil::digest_bytes(b"binary", crate::domain::HashAlgo::Sha256);
        assert!(
            install_verified_file(&source, &binary, &destination, &source_hash, &binary_hash)
                .is_ok()
        );
        std::fs::write(&binary, b"truncated").unwrap();
        assert!(
            install_verified_file(&source, &binary, &destination, &source_hash, &binary_hash)
                .is_err()
        );
        assert!(!destination.with_extension("partial").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reuses_only_validated_cache() {
        let dir = std::env::temp_dir().join(format!("winmoding-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cached = dir.join("cached.zip");
        std::fs::write(&cached, b"cached").unwrap();
        let hash = crate::hashutil::digest_bytes(b"cached", crate::domain::HashAlgo::Sha256);
        assert!(cached_valid(&cached, &hash));
        std::fs::write(&cached, b"corrupt").unwrap();
        assert!(!cached_valid(&cached, &hash));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_duplicate_manifest_assets_and_requires_complete_release_asset_set() {
        let manifest = serde_json::json!({
            "release_tag": "v1",
            "source_asset": "source.tar.gz",
            "source_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "source_commit": "source-commit",
            "steelmc_commit": "steel-commit",
            "executable_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "assets": [
                {"name": "source.tar.gz", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                {"name": "source.tar.gz", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                {"name": "win-client-windows-x64.zip", "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
            ]
        });
        assert!(parse_release_manifest(&manifest).is_err());

        let manifest = serde_json::json!({
            "release_tag": "v1",
            "source_asset": "source.tar.gz",
            "source_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "source_commit": "source-commit",
            "steelmc_commit": "steel-commit",
            "executable_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "assets": [
                {"name": "source.tar.gz", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
            ]
        });
        let contract = parse_release_manifest(&manifest).unwrap();
        let release_assets = vec![
            "source.tar.gz".to_string(),
            "win-client-windows-x64.zip".to_string(),
        ];
        assert!(validate_release_asset_set(&release_assets, &contract, "v1").is_err());
    }

    #[test]
    fn validates_corresponding_source_archive_markers() {
        let dir = std::env::temp_dir().join(format!("winmoding-source-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("source.tar.gz");
        std::fs::write(&archive, b"not-a-tarball").unwrap();
        assert!(validate_source_archive(&archive, "steel-commit", "source-commit").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn detects_tampered_installed_executable_before_launch() {
        let dir = std::env::temp_dir().join(format!("winmoding-launch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("win-client.exe");
        let mut trusted = vec![0u8; 68];
        trusted[0..2].copy_from_slice(b"MZ");
        trusted[0x3c..0x40].copy_from_slice(&64u32.to_le_bytes());
        trusted[64..68].copy_from_slice(b"PE\0\0");
        std::fs::write(&executable, &trusted).unwrap();
        let expected = crate::hashutil::digest_bytes(&trusted, crate::domain::HashAlgo::Sha256);
        assert!(verify_installed_client_for_launch(&executable, &expected).is_ok());
        std::fs::write(&executable, b"tampered").unwrap();
        assert!(verify_installed_client_for_launch(&executable, &expected).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_replacement_preserves_previous_executable_on_failure() {
        let dir = std::env::temp_dir().join(format!("winmoding-atomic-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("new.partial");
        let destination = dir.join("win-client.exe");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&destination, b"old").unwrap();
        atomic_replace(&source, &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!source.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
