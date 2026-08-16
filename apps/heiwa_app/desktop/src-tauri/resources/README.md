# Bundled resources

The release workflow stages the `heiwa` runtime binary here before running
`tauri build`, so the installed app carries the runtime it was built with
rather than depending on whatever happens to be on the user's `PATH` — or on
nothing at all.

The binary itself is never committed; this file exists so the bundle's
`resources/*` glob always matches, including in a plain `cargo build` where
no runtime has been staged.

Layout after install:

| Platform | Runtime lands at |
|---|---|
| macOS | `Heiwa.app/Contents/Resources/heiwa` |
| Windows | next to `Heiwa.exe` |
| Linux | next to the executable |

`runtime_supervisor::find_runtime_binary` searches those locations in that
order, falling back to `PATH` only for a development build with no bundle.
