import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { version } from "../../package.json";

export const allowState = ref(false);
export const installState = ref(false);
export const updateState = ref(false);
export const remoteVersion = ref("");
export const remoteDownloadUrl = ref("");
export const latestBetaCommitTruncatedSha = ref("");
export const latestBetaCommitLink = ref("");
export const launcherUrl = "https://github.com/vithorixscom-debug/WinModing";

const RELEASES_API = "https://api.github.com/repos/vithorixscom-debug/WinModing/releases";

/** Parse "v0.3.2" / "0.3.2-beta" into comparable numeric parts. */
export function parseVersion(raw) {
  const text = String(raw || "")
    .trim()
    .replace(/^[vV]/, "");
  const core = text.split(/[-+]/)[0] || "";
  return core
    .split(".")
    .map((part) => {
      const digits = String(part).replace(/[^\d].*$/, "");
      return Number.parseInt(digits || "0", 10) || 0;
    });
}

/** true only when remote is strictly newer than local (never for equal/older). */
export function isNewerVersion(remote, local) {
  const a = parseVersion(remote);
  const b = parseVersion(local);
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i += 1) {
    const left = a[i] || 0;
    const right = b[i] || 0;
    if (left > right) return true;
    if (left < right) return false;
  }
  return false;
}

function setReleaseLabel(text) {
  remoteVersion.value = text || "";
  if (typeof document === "undefined") return;
  for (const node of document.querySelectorAll("#releaseData")) {
    node.textContent = text || "—";
  }
}

async function fetchLatestRelease() {
  const rows = await invoke("catalog_get_json", { url: RELEASES_API }).catch(() => null);
  const list = Array.isArray(rows) ? rows : [];
  const release =
    list.find((row) => !row.draft && !row.prerelease && row.tag_name) ||
    list.find((row) => !row.draft && row.tag_name) ||
    null;
  if (!release) return null;
  const assets = Array.isArray(release.assets) ? release.assets : [];
  const exe =
    assets.find((a) => String(a.name || "").toLowerCase() === "winmoding.exe") ||
    assets.find((a) => String(a.name || "").toLowerCase().endsWith(".exe")) ||
    null;
  return {
    tag: String(release.tag_name || "").replace(/^[vV]/, ""),
    name: release.name || release.tag_name || "",
    url: exe?.browser_download_url || `${launcherUrl}/releases/latest`,
    htmlUrl: release.html_url || `${launcherUrl}/releases`,
  };
}

/**
 * @param {boolean} [showPrompt] unused — kept for call sites
 * @param {boolean} [doInstall] when true and a newer build exists, download/install it
 */
export async function getRemote(showPrompt = false, doInstall = false) {
  try {
    const remote = await fetchLatestRelease();
    if (!remote) {
      setReleaseLabel("unavailable");
      updateState.value = false;
      return;
    }
    setReleaseLabel(remote.tag.startsWith("v") ? remote.tag : `v${remote.tag}`);
    remoteDownloadUrl.value = remote.url || "";

    const newer = isNewerVersion(remote.tag, version);
    updateState.value = newer;
    allowState.value = newer;

    if (doInstall && newer && remote.url) {
      installState.value = true;
      try {
        await invoke("install_launcher_update", { url: remote.url });
      } finally {
        installState.value = false;
      }
    }
  } catch (err) {
    console.warn("update check failed", err);
    setReleaseLabel("unavailable");
    updateState.value = false;
  }
}

export async function getBranches() {
  try {
    const commits = await invoke("catalog_get_json", {
      url: "https://api.github.com/repos/vithorixscom-debug/WinModing/commits?per_page=1",
    }).catch(() => []);
    const head = Array.isArray(commits) && commits[0] ? commits[0] : null;
    if (!head) return;
    const sha = String(head.sha || "").slice(0, 7);
    latestBetaCommitTruncatedSha.value = sha;
    latestBetaCommitLink.value = head.html_url || `${launcherUrl}/commit/${head.sha || ""}`;
  } catch {
    /* ignore */
  }
}
