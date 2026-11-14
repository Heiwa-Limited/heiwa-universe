# Heiwa

Heiwa is a macOS workspace for Apple apps, personal workflows and AI agents, built on the MIT-licensed T3 Code foundation. See [HEIWA.md](HEIWA.md) for the product contract and [NOTICE](NOTICE) for the pinned upstream source.

Heiwa is a preview. There is no public installer or update feed; the app is built from source. Work lands on `dev`, and `main` is the last `dev` checkpoint that was installed and run on the owner's Mac. Calendar, Reminders, Notes and Mail workflows run on macOS and ask for review before they write, unless you turn on automatic changes for an app. The web, mobile and non-macOS surfaces come from the foundation and have not been verified for Heiwa.

## Development

Use the pinned Node and package manager versions with Vite+. Install workspace dependencies, then run `vp run dev --no-seed --home-dir <isolated-profile>`. Development in a linked worktree defaults to its own `.heiwa-dev` directory. Never point a development server at a live profile.

To install a build as `~/Applications/Heiwa.app`, run `vp run desktop:channel install`; see [Installing the dev or main channel](docs/operations/development.md#installing-the-dev-or-main-channel).

The runtime owns durable work and side effects. The app, CLI and MCP tools share its services. Product analytics is disabled by default. Upstream service accounts, update feeds and private credentials are not part of this fork.

## License

The T3 Code foundation is MIT licensed, copyright 2026 T3 Tools, Inc. Attribution and third-party notices are preserved. Heiwa changes are maintained by Heiwa Limited.
