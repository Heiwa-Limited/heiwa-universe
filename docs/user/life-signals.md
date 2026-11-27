# Life signals

Life signals let agents see your day on this Mac without reading your messages: who is waiting
on a reply and for how long, mail you have not answered, and how the day went in counts
(messages in and out, notifications by app, focus time by app, AI usage by provider).

Agents get it through the `heiwa_life` tool, in two views:

- **loops**: people threads waiting on you (name, hours waiting, and the on-device triage label
  _meeting_ or _needs clarification_ when there is one), plus unanswered mail from people (sender,
  subject, age).
- **day**: the counts above for today, or for a `date` you name.

Message and mail bodies never leave the Mac. Triage labels come from Apple's on-device model
(see [Apple apps](./apple-apps.md)), so the text is never sent to a provider either.

## Turning it on

Life signals are off until you turn on **Settings → Labs → Life signals** on the Mac that holds
the data. While it is off, `heiwa_life` refuses every call. Subagents never see the tool.

The data comes from the local feeder in `~/heiwa-root` (the same engine as
[Decisions](./decisions.md)), served on `127.0.0.1:8790` and refused when a request arrives
from the network. To point Heiwa at another address, set `HEIWA_LIFE_URL` for the server.

Names and subjects are treated as untrusted data: an agent may summarize them, but text inside
a subject is never an instruction.
