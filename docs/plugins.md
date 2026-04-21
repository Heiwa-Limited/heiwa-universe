# Plugins

## Current scope

Heiwa does not have a plugin marketplace.

The current install protocol is a narrow GitHub source address for pulling a plugin repository into the local runtime root:

```bash
heiwa install gh:owner/repo
heiwa install gh:owner/repo@v1.2.3
```

This is intentionally small:

- `gh:` means "clone from GitHub over HTTPS"
- `owner/repo` identifies the repository to install
- `@ref` is optional and may be a branch, tag, or commit that `git checkout` can resolve

## Install layout

Heiwa installs repositories under the owner-local runtime root:

```text
~/.heiwa/plugins/github.com/<owner>/<repo>/
```

Each installed repo also gets a runtime-owned receipt file:

```text
~/.heiwa/plugins/github.com/<owner>/<repo>/.heiwa-install.json
```

That receipt records:

- the canonical source string
- the resolved HTTPS clone URL
- the install path
- the install timestamp

## Behavior

`heiwa install` now has two modes:

- `heiwa install`
  Bootstraps `~/.heiwa/`, writes the canonical launcher, refreshes `machine.json`, and registers the device when a backend connection is available.
- `heiwa install gh:owner/repo[@ref]`
  Clones a GitHub repository into `~/.heiwa/plugins/...` and writes the local receipt file. It does not register a device or execute plugin code during install.

## Constraints

- `git` must be available on the local machine.
- The target repository must be reachable with the operator's existing GitHub auth posture.
- Reinstall/update semantics are not part of this slice yet. If the target path already exists, install fails instead of mutating an existing checkout.
- This protocol only standardizes acquisition and local placement. Activation, sandboxing, permission models, and plugin runtime contracts remain future work.

## Non-goals

- No hosted plugin marketplace
- No opaque registry IDs
- No claim that provider-native plugin systems are replaced
- No automatic background updates

Providers still own their own plugin, MCP, and extension mechanics. This protocol is only the Heiwa-side way to fetch a GitHub-hosted extension repo into the local runtime root.
