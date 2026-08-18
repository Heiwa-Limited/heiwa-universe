# Tauri v2 Updater — Heiwa Implementation Reference

> Implementation reference for shipping an auto-updating `Heiwa.app` with **no Apple
> Developer Program membership**. Fetched from the Tauri v2 docs on 2026-08-18.

## Why this works without an Apple certificate

Two signing systems are being confused whenever "we need to pay Apple to auto-update" comes up:

| System | Purpose | Cost | Needed? |
|---|---|---|---|
| Apple Developer ID + notarization | clears macOS Gatekeeper on **browser-downloaded** files | $99/yr | **no** |
| Tauri minisign keypair | proves the update payload came from us | free | **yes** |

Gatekeeper's first-launch block keys on the `com.apple.quarantine` extended attribute,
which is set by the *downloading application* — browsers, Mail, AirDrop. Verified on this
machine 2026-08-18: an asset fetched with `curl` from GitHub Releases carries only
`com.apple.provenance`, **no quarantine bit**.

The updater writes the new bundle itself, so updater-delivered builds are never quarantined
either. The consequence: **deliver via `curl` and via the updater, never via a browser
download link.** Publishing a `.dmg` on the releases page reintroduces the exact problem the
$99 would have solved.

Keep `"signingIdentity": "-"` in `tauri.conf.json` — ad-hoc signing is required on Apple
Silicon and already satisfied.

## Manifest format

Static JSON, served from any URL:

```json
{
  "version": "0.2.0",
  "notes": "Update description",
  "pub_date": "2026-08-18T10:30:00Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "<contents of the .sig file, not a path>",
      "url": "https://github.com/Heiwa-Limited/heiwa-universe/releases/download/v0.2.0/Heiwa.app.tar.gz"
    },
    "linux-x86_64": { "signature": "...", "url": "https://.../heiwa.AppImage" },
    "windows-x86_64": { "signature": "...", "url": "https://.../heiwa-setup.exe" }
  }
}
```

Required: `version`, `platforms.<target>.url`, `platforms.<target>.signature`.
The signature field holds the **contents** of the `.sig` file.

macOS updater artifacts are `Heiwa.app.tar.gz` + `Heiwa.app.tar.gz.sig` — the same
`.app`-in-a-tarball shape the public installer wants, so one artifact serves both paths.

## Keypair

```bash
npm run tauri signer generate -- -w ~/.tauri/heiwa.key
```

Public key goes in `tauri.conf.json`. Private key goes in CI secrets as
`TAURI_SIGNING_PRIVATE_KEY`. **Losing the private key ends the ability to ship updates to
every installed copy** — it has no recovery path and belongs in the same custody class as
a release signing root.

## Config

```json
{
  "bundle": { "createUpdaterArtifacts": true },
  "plugins": {
    "updater": {
      "pubkey": "<content of the generated .pub file>",
      "endpoints": ["https://heiwa.ltd/updates/{{target}}/{{arch}}/{{current_version}}"]
    }
  }
}
```

Capability grant in `src-tauri/capabilities/default.json`:

```json
{ "permissions": ["updater:default"] }
```

`updater:default` covers `allow-check`, `allow-download`, `allow-install`,
`allow-download-and-install`.

## Dependencies

```bash
cargo add tauri-plugin-updater --target 'cfg(any(target_os = "macos", windows, target_os = "linux"))'
npm install @tauri-apps/plugin-updater
```

Plugin registration in `lib.rs`:

```rust
tauri::Builder::default()
  .setup(|app| {
    #[cfg(desktop)]
    app.handle().plugin(tauri_plugin_updater::Builder::new().build());
    Ok(())
  })
```

## Build

Signing env vars must be exported — `.env` files are not read:

```bash
export TAURI_SIGNING_PRIVATE_KEY="path-or-content"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npm run tauri build
```

## Rust API

```rust
use tauri_plugin_updater::UpdaterExt;

if let Some(update) = app.updater()?.check().await? {
    update.download_and_install(|_chunk, _total| {}, || {}).await?;
    app.restart();
}
```

## Relationship to `heiwa app update`

The runtime already has a verified update path (`apps/heiwa_shell/src/cmd/release_update.rs`,
commit `0e6d47e5`): fetches from GitHub release assets only, verifies SHA-256 against the
published checksums file, refuses archives containing links or paths outside the expected
root, stages beside the destination, lands with an atomic rename, and never restarts on its
own.

These are two halves of one story and must not diverge:

- `heiwa app update` — the **runtime** binary updates itself. Trust anchor: SHA-256 against
  the release checksum manifest.
- `tauri-plugin-updater` — the **shell bundle** updates itself. Trust anchor: minisign
  signature over the payload.

Both read the same GitHub release. Whichever cuts a release must publish both the checksum
manifest and the signed updater manifest, or one half silently stops advancing.

**Known blocker (2026-08-18):** the released v0.1.0 binary predates `0e6d47e5` and reports
`"implemented": false` from `heiwa app update --dry-run --json`. Every installed copy is
stranded until v0.2.0 ships.

## Sources

- https://v2.tauri.app/plugin/updater/
