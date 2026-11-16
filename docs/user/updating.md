# Updating Heiwa

## Update this Mac

In a managed macOS preview, open **Settings → About**. Choose the channel to follow:

- **Development (dev)** gets the latest integrated work before public promotion.
- **Stable (main)** gets checkpoints promoted for public installation.

Choose **Check for Updates**, then **Update & Restart**. Confirm after finishing active
work. Heiwa builds the selected version on this Mac, closes to replace the app, and
reopens. The selected channel is saved after a successful installation. Switching to
another channel follows the same process.

This preview uses a local source checkout and build tools. The first installation must
be prepared by the maintainer; if updates are unavailable, the page explains why.
There is no public binary update feed yet.

## Go back to the previous app

Choose **Roll Back** on the same page. Heiwa keeps the previous installed app and checks
whether it can use your current database. If the database is incompatible, the operation
stops. The app never silently restores older data. Rolling back the app leaves your
followed channel unchanged.

## If an update fails

The update status shows the error. A failed build does not replace the installed app.
An interrupted operation is reported when you reopen Settings. Check for updates again
after fixing the reported problem. If another installer is running, let it finish before
retrying.

Before installing a version that migrates the database, Heiwa takes a database snapshot.
This is not a backup of attachments, provider state or all settings. Channel switching
uses the same local profile.

## Update providers

**Settings → Providers** offers updates for supported provider CLIs in the selected
environment. These are separate from the Heiwa app update. An existing agent session
can keep using its previous CLI until it ends. Refresh provider status if discovery
fails after an update.

Updating this Mac does not update remote machines. Keep connected clients and servers
on compatible versions and update each host through its own supported installation flow.
