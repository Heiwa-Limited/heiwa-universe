# Decisions

Decisions are questions only you can answer, prepared on your Mac from Messages, Mail,
Calendar, Reminders and your holdings: a person waiting on a reply, an overdue reminder,
a calendar clash, an unusual move in something you hold. They appear at the top of
**Approvals**, on every device connected to that Mac.

Each decision is an [A2UI](https://a2ui.org/specification/v1.0-a2ui/) card with its
evidence and two or three choices. Click a choice, or focus the card and press **1–9** to
pick the matching button. Answers that would change an Apple app (completing or moving a
reminder) are not applied directly: they wait for your approval first.

## Turning it on

Decisions come from a local engine on the Mac (`~/heiwa-root`, served on
`127.0.0.1:8790`). Heiwa reads it through the server, so it is never reachable from the
network. When the engine is not running, the section simply does not appear. To point
Heiwa at a different engine address, set `HEIWA_DECISIONS_URL` for the server.

Only the owner sees decisions: they use the same permission as approving Apple changes.
