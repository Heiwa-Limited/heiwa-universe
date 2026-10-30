# Heiwa

Heiwa is a macOS-first workspace for personal work, Apple apps and AI agents. This application is derived from the MIT-licensed T3 Code foundation; see NOTICE and LICENSE.

The server owns work and side effects. Its durable event log, projections, command receipts and outbox commit together. Clients present that state; provider adapters translate provider-owned protocols. A request being accepted does not mean its work completed.

Apple operations and packaged workflows are shared services used by the app, agents and CLI. Native helpers run outside the main process with deadlines. Permission, resource selection, intended changes and persisted outcomes remain visible. Signing into a provider does not itself prove model entitlement or successful execution.

Heiwa's identity, data and update authority are independent from upstream T3. Preserve upstream attribution while using Heiwa's own consent and account-registration identity. Product telemetry is off unless a Heiwa endpoint and an explicit opt-in are configured.

The canonical desktop installation is `~/Applications/Heiwa.app`. Development and migration use isolated profiles. Preserve old state until a verified migration and replacement exist; do not run two authorities against one live profile.

Build beyond coding: conversations must support Apple-native productivity, local models, reusable workflows and code projects without requiring users to understand provider protocols or implementation details.
