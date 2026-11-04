# Win Rust Minecraft Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a separately hosted, Windows-buildable Rust Minecraft client branded Win and integrate opt-in download, install, profile creation, and launch into WinModing.

**Architecture:** Preserve the existing `win-client/` upstream source snapshot and its provenance while rebranding product-facing identity and making a distributable Win build. Publish client source and release assets in a separate `vithorixscom-debug` repository. WinModing consumes release metadata and a Windows executable asset, verifies SHA-256 before installation, and only downloads it during Win profile creation; Java flow stays unchanged.

**Tech Stack:** Rust workspace/Cargo, pinned Rust toolchain, Vulkan, Python build helpers, GitHub Actions/Releases, existing WinModing Tauri 2 + Rust backend and Vue/Vite frontend, Rust and Vitest tests.

**Spec:** `docs/superpowers/specs/2026-10-30-win-client-integration-design.md`

## Global Constraints

- Separate repository copy, not a fork; target GitHub account is `vithorixscom-debug`.
- Win client is downloaded only when a user creates a Win profile; Java profile behavior and downloads are unchanged.
- Preserve original Pomme authorship and comply with GPL-3.0-or-later, applicable SteelMC AGPL-3.0-or-later, MIT, and other third-party license obligations.
- Do not ship Mojang/Microsoft game assets or imply upstream endorsement.
- Use WinModing-owned branding only where ownership/licensing permits; remove Pomme branding unless authorized.
- Verify the downloaded release asset's SHA-256 before installing or executing it.
- Never create a successful/half-installed profile after a failed Win download; clean partial files and allow retry.
- Do not overwrite or discard existing uncommitted changes in either working tree.
- Do not publish until Windows build/startup, integration, and license/source checks pass; do not claim publication if credentials or repository permissions are unavailable.

## Review Focus

- GitHub release missing the expected Windows asset, checksum, or valid metadata → release-parser tests in Task 3 and workflow validation in Task 6.
- Truncated, corrupt, or checksum-mismatched executable → download/install tests in Task 3.
- Network failure or cancellation during Win profile creation → cleanup/no-profile/retry tests in Task 4.
- Existing cached version or legacy `pomme`/`horizon` profile → reuse/normalization/launch tests in Task 4.
- Java profile creation or launch accidentally routed through native client code → Java regression tests in Task 4.

---

## File Map

### Client source/release repository (`win-client/` before it is copied to the new remote)
- `win-client/README.md`, `BUILDING.md`, `CONTRIBUTING.md`: Win product documentation and attribution.
- `win-client/Cargo.toml`, Rust toolchain and package metadata: executable/package identity and Windows build configuration.
- `win-client/pomme-client/**`, `win-client/pomme-launcher/**`, and other workspace crates: product-facing strings, app IDs, data directory names, icons, and launch contract; change only after source audit identifies each consumer.
- `win-client/LICENSE`, `THIRD_PARTY_LICENSES.md`, `win-client/third_party/SteelMC/**`: preserve/extend notices and corresponding source obligations; never replace upstream copyright.
- `win-client/.github/workflows/**`: reproducible Windows build, artifact, checksum, and release validation.

### WinModing repository (root)
- `src-tauri/src/native_clients.rs`: release metadata, asset selection, cache path, download integrity, install/launch safety.
- `src-tauri/src/commands/native_clients.rs`, `src-tauri/src/lib.rs`: Tauri command contract and create/install/launch orchestration.
- `src/components/ui/InstanceCreationModal.vue`, `src/helpers/i18n.js`: Win description, Create progress and error UX.
- `src/helpers/nativeClients.js`, `src/helpers/profile.js`, `src/pages/instance/Index.vue`: frontend bridge and launch behavior.
- `src-tauri/src/native_clients.rs` tests and/or focused integration tests; frontend tests under `src/**` following existing Vitest conventions.
- `docs/superpowers/specs/2026-10-30-win-client-integration-design.md` and `docs/superpowers/plans/2026-10-30-win-client-integration.md`: design and plan.

## Task 1: Audit client provenance, build, branding, and redistribution boundary

**Files:**
- Read: `win-client/README.md`, `BUILDING.md`, `Cargo.toml`, `THIRD_PARTY_LICENSES.md`, `LICENSE`, `.gitmodules`, `rust-toolchain.toml`, Windows packaging and client entry point.
- Create: `win-client/docs/WIN-DISTRIBUTION.md`

- [ ] Map the Windows executable entry point, arguments, data directory, assets, graphics/audio runtime dependencies, build recipes, and exact release output.
- [ ] Inventory direct and transitive bundled code/licenses, determine whether SteelMC is built into the Windows release, and document source/offer obligations and how the matching corresponding source will be provided.
- [ ] Document upstream provenance (PommeMC/Client URL and inspected revision), original author/copyright, licenses, WinModing-owned brand asset provenance, and non-endorsement statement.
- [ ] Identify every product identity string/app ID/data path/icon that must become Win and every third-party notice that must remain untouched.
- [ ] Verify target repository name availability and the configured GitHub CLI/auth identity read-only; report a blocker rather than creating/pushing anything if unavailable.
- [ ] Deliver the audit note as the gate for subsequent redistribution changes; do not make product-code edits in this audit task.

## Task 2: Rebrand and make a standalone Windows Win client build

**Files:**
- Modify: audited identity/configuration files under `win-client/` (exact list recorded by Task 1).
- Modify: `win-client/README.md`, `BUILDING.md`, package metadata and Windows packaging configuration.
- Modify: license/third-party/source-offer documentation only as required by Task 1.
- Test: client workspace checks and Windows release build.

- [ ] Add/adjust tests or static checks that assert product-facing package/app identity and default data directory use Win, not Pomme.
- [ ] Apply Win branding and the WinModing-owned icon where permitted; retain original attribution and all license headers/notices.
- [ ] Configure the Windows executable name and arguments/data layout to be stable and explicit for launcher consumption; do not remove capabilities required by the agreed client.
- [ ] Build the release executable using the pinned toolchain and documented Vulkan/Python prerequisites; record command, artifact name, architecture, and runtime requirements.
- [ ] Run the client on Windows and verify initial asset bootstrap, Microsoft sign-in/game entry where available, server join, graceful failure without supported Vulkan/runtime, and that it does not open WinModing.
- [ ] Run the client's formatting, lint, and test/pre-PR recipes; retain logs/results for the release gate.

## Task 3: Implement safe WinModing release download and installation

**Files:**
- Modify: `src-tauri/src/native_clients.rs`.
- Test: focused Rust tests in the same module or `src-tauri/tests/native_clients.rs`.

**Interfaces:**
- Produces `NativeClientRelease` metadata with client `win`, tag/version, release URL, and optional installed path.
- Installer accepts the configured repo and client, resolves an exact Windows asset plus SHA-256 from release metadata, downloads to a temporary file, verifies before atomic placement, and returns the installed executable path or a user-readable error.

- [ ] Write failing tests for valid metadata/asset selection, absent asset, absent/malformed checksum, and non-Windows/unsupported asset selection.
- [ ] Write failing tests for matching SHA-256 success, checksum mismatch rejection, truncated response, and partial-file cleanup.
- [ ] Replace hard-coded launcher asset assumptions with the new standalone repository/release contract and the exact artifact name chosen in Task 2.
- [ ] Require a supported checksum field and verify the complete downloaded bytes before atomic install; never execute unverified bytes.
- [ ] Preserve cached validated client versions and avoid downloading during ordinary Java flows or every game launch.
- [ ] Run focused Rust tests and `cargo test` in `src-tauri`; confirm unrelated pre-existing test failures are identified separately.

## Task 4: Integrate Win profile creation and game launch without Java regressions

**Files:**
- Modify: `src-tauri/src/commands/native_clients.rs`, `src-tauri/src/lib.rs`.
- Modify: `src/components/ui/InstanceCreationModal.vue`, `src/helpers/i18n.js`, `src/helpers/nativeClients.js`, `src/helpers/profile.js`, `src/pages/instance/Index.vue`.
- Test: Rust native-client/profile tests and frontend Vitest tests following local conventions.

**Interfaces:**
- Tauri create command returns the persisted Win profile only after a verified client is installed and the profile/icon/options are ready.
- Native launch command accepts profile/client and optional server, validates the installed executable and arguments, and returns the launched game process ID.

- [ ] Write failing backend tests for Win install success then profile persistence, download failure then no profile, invalid executable then no launch, icon presence, and server argument validation.
- [ ] Write failing frontend tests for Win description, create loading/error/retry state, and unchanged Java profile selection/create behavior.
- [ ] Wire Create to install/cache Win, then save profile; surface progress and actionable errors, clean partial downloads, and preserve icon behavior.
- [ ] Launch only the installed game client; reject accidental WinModing launcher binaries using a robust artifact identity/manifest check established in Task 1/2, not fragile PE-section-name heuristics.
- [ ] Preserve legacy Pomme/Horizon profile normalization to Win where applicable, without exposing separate product choices.
- [ ] Run `cargo test`, `npm test -- --run`, `npm run build`, and existing relevant Rust checks.

## Task 5: Prepare the separate repository copy and safe release workflow

**Files:**
- Create/configure: new repository `vithorixscom-debug/Win-Client` (exact name may be adjusted only if unavailable, preserving clear separate identity).
- Create/modify: `.github/workflows/windows-release.yml` in the client repository.
- Include: full source, pinned toolchain/build instructions, licenses, third-party notices, source correspondence/offer materials required by Task 1.

- [ ] Create a separate source copy with a documented upstream revision/provenance; do not make a fork and do not push the nested clone's `.git` metadata as a subdirectory.
- [ ] Add CI for reproducible Windows x64 build, tests/lints, packaging, SHA-256 generation, and validation that executable/checksum/metadata are all present.
- [ ] Publish release assets with stable name and a machine-readable manifest (version, platform, architecture, asset name, SHA-256, release/source URL).
- [ ] Include the matching source and required license notices/source offers alongside each distributed binary as required by the completed license audit.
- [ ] Validate the workflow on a draft/pre-release or CI artifact before publishing a public release.
- [ ] Obtain explicit final review of source contents, license package, and artifact before creating a public release; do not expose secrets or unrelated workspace files.

## Task 6: End-to-end validation and publish gate

**Files:**
- Modify as needed: client release workflow and WinModing integration/tests.
- No destructive or public operation without final artifact review.

- [ ] On a clean Windows environment, create a Win profile from the launcher and verify the client is fetched once, checksum passes, profile icon/name appear, and Minecraft launches; test repeat creation/launch uses cache.
- [ ] Test failed network, missing GitHub asset, checksum mismatch, corrupted download, unsupported Vulkan/runtime, and retry; verify no false successful profile and useful error text.
- [ ] Create and launch a Java profile before/after the change and verify it has no Win-client download or behavior regression.
- [ ] Verify the client repository license/source bundle and release contents against Task 1 audit; inspect Windows binary metadata and SHA-256 independently.
- [ ] Run final `cargo check`, `cargo test`, `npm test -- --run`, `npm run build`, client pre-PR/build commands; record failures/warnings accurately.
- [ ] Review diffs in both repositories, ensuring existing user modifications remain untouched and no build artifacts, credentials, Mojang assets, or irrelevant files are included.
- [ ] Publish the separate source repository and Windows release only after all gates pass and credentials authorize the operation; then verify public release metadata/assets and perform an actual WinModing download smoke test.

## Coverage self-review

- Design goals (branding, description, automatic opt-in download, checksum, profile/launch, Java unaffected) map to Tasks 2–4.
- Separate repository, Windows CI/release, and source/legal obligations map to Tasks 1, 5, and 6.
- Failure/retry and cache behavior map to Tasks 3, 4, and 6.
- Public publication is explicitly gated in Task 6; plan does not assume credentials or license clearance.
