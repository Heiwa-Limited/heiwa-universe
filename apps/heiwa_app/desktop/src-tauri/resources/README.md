# Bundled resources

The release workflow stages the `heiwa` runtime binary here before running
`tauri build`, so the installed app carries the runtime it was built with
rather than depending on whatever happens to be on the user's `PATH` — or on
nothing at all.

The binary itself is never committed; this file exists so the bundle's
`resources/*` glob always matches, including in a plain `cargo build` where
no runtime has been staged.

Layout after install. The runtime lands in the platform's resource directory,
which is not always beside the executable, and the bundler keeps the staged
`resources/` prefix:

| Platform | Resource directory | Runtime lands at |
|---|---|---|
| macOS | `Heiwa.app/Contents/Resources` | `…/Resources/resources/heiwa` |
| Windows | beside `Heiwa.exe` | `…/resources/heiwa.exe` |
| Linux (deb, AppImage) | `/usr/lib/Heiwa` | `/usr/lib/Heiwa/resources/heiwa` |

`runtime_supervisor::find_runtime_binary` asks Tauri for that directory rather
than hardcoding the layouts — on Linux the executable installs to `/usr/bin`,
so nothing is beside it — and falls back to executable-relative paths only
when the resource directory cannot be resolved, then to `PATH` for a
development build with no bundled runtime.
