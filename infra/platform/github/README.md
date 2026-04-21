# GitHub Release Contract

This directory documents the repo-native distribution contract used by the platform lane.

## Release Trigger

- Git tags matching `v*`
- Manual rerun through `.github/workflows/release.yml` with an existing tag

## Published Assets

The release workflow publishes one archive per platform:

- `heiwa-<version>-linux-x86_64.tar.gz`
- `heiwa-<version>-macos-aarch64.tar.gz`
- `heiwa-<version>-windows-x86_64.zip`
- `heiwa-<version>-checksums.txt`

Each platform archive contains:

- `heiwa` or `heiwa.exe`
- `README.md`
- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`

## Downstream Use

- P3 Homebrew tap should source the macOS tarball and checksum from this contract.
- Future install scripts should rely on the same asset names instead of inventing new archive formats.
