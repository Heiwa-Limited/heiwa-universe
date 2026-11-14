# Heiwa

Heiwa is being rebuilt as a macOS workspace for Apple apps, personal workflows and AI agents, on the MIT-licensed T3 Code foundation. See [HEIWA.md](HEIWA.md) for the product contract and [NOTICE](NOTICE) for the pinned upstream source.

This branch is a development rebuild. It is not yet a replacement for the installed Heiwa application. Native Apple workflows and local inference are being integrated and must pass end-to-end acceptance before installation.

## Development

Use the pinned Node and package manager versions with Vite+. Install workspace dependencies, then run `vp run dev --no-seed --home-dir <isolated-profile>`. Development in a linked worktree defaults to its own `.heiwa-dev` directory. Never point a development server at a live profile.

The runtime owns durable work and side effects. The app, CLI and MCP tools share its services. Product analytics is disabled by default. Upstream service accounts, update feeds and private credentials are not part of this fork.

## License

The T3 Code foundation is MIT licensed, copyright 2026 T3 Tools, Inc. Attribution and third-party notices are preserved. Heiwa changes are maintained by Heiwa Limited.
