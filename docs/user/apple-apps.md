# Apple apps and approvals

Heiwa can read and change Calendar, Reminders, Notes and Mail on a Mac, through the agents you
chat with. You choose what it may touch, and by default you approve each change before it happens.

## Connect an app

Open **Settings > Apple apps** and choose **Connect** for an app. macOS asks for permission the
first time. Then tick the calendars, lists, folders or mailboxes Heiwa may use. Anything you leave
unticked stays invisible to agents. Mail also needs Full Disk Access for Heiwa in System Settings.

## Approve changes

When an agent prepares a change, Heiwa notifies you and lists it under **Approvals** in the
sidebar. Each entry shows the exact change: the event and its time, the reminder, the note text,
or the mail draft and its recipients. **Approve change** applies it and reads the item back to
confirm it. **Reject** discards it.

Approval covers only the change shown. If the item changed in the meantime, Heiwa refuses and the
agent has to prepare a fresh change.

If Heiwa cannot confirm what happened, the entry stays in Approvals as uncertain. Check the item
in its Apple app, then record what you found. Heiwa never repeats an uncertain change on its own.

## Let routine changes through

Under an app in **Settings > Apple apps**, **Allow routine changes** lets an agent apply changes
without asking. It applies only when all of these hold:

- the chat has Full access,
- you typed the message the agent is working on.

Scheduled tasks, webhooks, agents started by other agents and turns that resumed on their own can
still only prepare changes for you to approve. Removing an item and replacing a note's text always
need approval.

## Watch your inbox

With a mailbox and a calendar selected, tick **Watch my inbox** under Mail in **Settings > Apple
apps**. Every 15 minutes Heiwa checks the selected mailboxes for new unread mail. When some
arrives, a local model reads it and prepares a calendar event for each message that asks to meet.
The events wait in **Approvals**, grouped under the email they came from. Nothing is added to your
calendar until you approve it.

A quiet inbox starts no chat and uses no model. Each message is looked at once, and the first check
goes back one day. Untick the box to pause. The watch is an ordinary scheduled task named **Mail
intake**: change how often it runs, its instructions or its model in **Settings > Scheduled tasks**.

It needs a local model connected under **Settings > Providers**.

## Limits

- Heiwa saves mail as drafts. It never sends mail.
- Recurring events, events with invitees, notes with attachments and very long notes must be
  edited in their own app.
- Apple apps are available on Mac environments only.
