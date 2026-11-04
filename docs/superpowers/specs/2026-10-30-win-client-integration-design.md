# Win Rust Minecraft Client — Design

**Status:** Draft — awaiting user approval  
**Date:** 2026-10-30

## Intent

Provide an optional native Rust Minecraft: Java Edition client branded as Win and integrated into WinModing. A user who selects Win and presses **Create profile** should have the required Windows client downloaded and installed automatically from a separate GitHub repository in `vithorixscom-debug`, then be able to launch the game from that profile. Users who do not choose Win must not download the client; existing Java profiles and behavior remain unchanged.

## Goals

- Rebrand the existing Pomme client as Win without presenting Pomme or Horizon as separate client choices.
- Build a Windows client compatible with the WinModing profile and launch flow.
- Publish the client source and Windows releases in a separate repository under `vithorixscom-debug`.
- Explain in the Win profile creation UI that this is a native Rust Minecraft client.
- On Create, download/install the appropriate release, verify its SHA-256, and only then save the Win profile; show actionable errors and permit retry after failures.
- Cache the installed client so it is not downloaded on every launch. Java-only users are unaffected.
- Use WinModing-owned branding/icon where useful; do not reuse Pomme identity assets without authorization.
- Preserve attribution, license texts, notices, and source/distribution obligations for all included components.

## Non-goals

- Reimplement Minecraft or distribute Mojang/Microsoft game assets, account credentials, or copyrighted game files in the client release.
- Change Java profile creation, installation, or launch behavior.
- Publish a release before the Windows build, compatibility, and licensing checks pass.
- Remove upstream attribution or imply the upstream authors endorse Win.

## Approaches considered

1. **Separate repository copy (selected):** copy the existing Pomme source into a new `vithorixscom-debug` repository, retain provenance and all license notices, replace product-facing identity with Win/WinModing branding, and publish Windows client assets there. This matches the user's explicit choice and keeps game-client source/releases separate from the launcher. The repository URL/name must be checked for availability before creation.
2. **Fork the upstream repository:** rejected because the user explicitly requested a separate copy rather than a fork.
3. **Put client releases in the WinModing repository:** rejected because the user wants the game client maintained separately and downloaded only by users creating a Win profile.

## Constraints and compliance

- The existing client source is in `win-client/`, an independent clone of `PommeMC/Client` at the time of inspection; do not mistake its nested `.git` metadata for the parent repository.
- Upstream README identifies Pomme as GPL-3.0-or-later, copyright Purdze. `pomme-singleplayer` links SteelMC, identified as AGPL-3.0-or-later; `pomme-block` and `pomme-gpu-allocator` are MIT. `THIRD_PARTY_LICENSES.md` records additional dependencies.
- Before redistributing binaries, inventory what is included and satisfy GPL/AGPL and third-party license obligations, including corresponding source availability and notices as applicable. Preserve copyright and attribution. Do not use Pomme name/artwork in Win product branding absent permission. This is an engineering compliance gate, not legal advice.
- Reuse only WinModing-owned icon/branding assets where their existing licensing/ownership permits it.
- Client startup depends on Windows graphics/Vulkan support and runtime dependencies. Validate on a clean Windows environment and provide clear diagnostic errors for unsupported systems or missing dependencies.
- Do not overwrite the user's existing uncommitted root-repository changes. Treat `win-client/` as a separately sourced tree and preserve its provenance/attributions.
- Do not create, push to, or publish a GitHub repository/release without checking repository availability, credentials, ownership, and release contents. User has authorized the target account, not permission to bypass unavailable credentials or destructive operations.

## User experience and integration

1. Selecting Win in the profile dialog displays a concise description: native Rust Minecraft client, downloaded automatically when creating this profile.
2. Pressing **Create profile** starts a visible progress state; WinModing queries the separate repository's release metadata, downloads the Windows asset to a temporary path, checks the expected SHA-256 and file format, then atomically installs/caches it.
3. Only after successful installation does WinModing persist the profile and icon. Failure leaves no successful/half-created profile, presents the cause and retry path, and cleans up partial downloads.
4. Launching a Win profile runs the cached game client with supported profile/server arguments; it must not launch the WinModing launcher itself. Java profiles continue through their current launch path.
5. Release metadata (version, asset name, platform, checksum) is explicit and validated. Updates use a documented policy and do not silently re-download on every game launch.

## Implementation decisions left to the builder

- Check availability and choose a clearly named repository such as `vithorixscom-debug/Win-Client` (user approved the account, not a specific repo name).
- Determine the pinned toolchain, Windows build prerequisites, CI/release method, asset packaging, and compatibility bridge based on the actual source and WinModing APIs.
- Audit whether the AGPL SteelMC component is enabled/included in the Windows build and comply with the applicable source obligations; do not disable required functionality merely to avoid disclosure without evaluating product impact.
- Add/adjust tests for release metadata parsing, checksum verification, install failure cleanup, profile creation sequencing, and regression-safe Java behavior.
- Verify Windows build, client startup/game connection, launcher install/launch path, UI tests, and licensing/source package before any release.

## Open gates

- User approval of this design is required before implementation.
- GitHub publication is a later gated action: validate the target repository and release artifacts first, then report any permission, legal, or platform blocker before publishing.
