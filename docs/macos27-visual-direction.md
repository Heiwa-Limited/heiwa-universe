# Installed desktop visual direction — September 11 revision

The user rejected the first installed sidebar/Home revision. Passing runtime tests
did not satisfy visual acceptance. This direction supersedes that revision's UI
choices; preserve working domain behavior, not its old composition.

Visual thesis: a calm, substantial macOS work home with charcoal materials, warm
white text, a restrained sage accent, and the proportions of the earlier native
study. Readable session rows and one persistent chatbar organize the app.

Reference: `HeiwaDesign.swift` in the conversation's `native-v3` study; its actual
window was inspected. The light study's structure is useful, but the production
app stays dark. Do not copy its fixture conversations, provider responses, Apple
resources or separate state database.

## Composition

1. A 248px sidebar: a generous traffic-light area, lowercase Heiwa wordmark with
   a coherent line icon, 38px navigation rows, Home / All sessions / Calendar /
   Mail, clear project groups and standalone sessions. Search filters the real
   catalog. Detailed lifecycle controls remain in menus. Resources live at foot.
2. A 56px toolbar across the work area with current location/session title and one
   New session action. No repeated primary action on Home. The window defaults
   to approximately 1280 x 840 and supports smaller usable widths.
3. Home follows the native study: a small date/section line, a 32px conversational
   heading, then full-width recent session rows with titles and bounded real
   metadata. Projects appear below as flat rows. A clean empty state is designed
   intentionally; blank sessions are not fabricated activity. Calendar details
   appear only when there is useful loaded content, not an empty dashboard box.
4. The composer is substantial: current conversation/location above, a 64px
   minimum input surface, 16px input text, coherent send icon, generous padding,
   and a restrained footer. Compact replies share the same visual system and
   align with the composer. Current-view labels must not claim context was sent
   until the runtime transport actually includes it.
5. Sessions, project detail, Calendar and Mail share toolbar, spacing and type.
   Calendar uses a clean continuous grid instead of separate rounded boxes for
   each day. Mail uses readable list/detail structure and truthful resource setup.

## Tokens and behavior

Use SF/system text, 15px main text, 13–14px navigation/metadata, meaningful contrast
even for secondary text. Charcoal background approximately #171a1c, sidebar
#1d2022, elevated controls #252a2c, text #edf0eb, secondary #a5b0a9, sage accent
#bfd6ab. Keep color literals in tokens. Remove the default automatic light-theme
switch for this requested dark-only direction. Buttons have 32px minimum targets,
visible focus and predictable menus. Use consistent SVG line icons instead of
assorted Unicode symbols. Motion is short and functional; honor reduced motion.

Real functionality must survive: catalog creation, rename/move/archive/restore,
draft isolation, provider setup, stream switching/cancellation, and response
expansion. All sessions and project navigation must open real runtime records.
Do not add decorative buttons, fake statistics, invented provider capability,
or marketing copy to fill empty space.

Acceptance is the actual installed native window at its default size and a
smaller size, including Home, a session, Calendar, a context menu and the response
panel. Review hierarchy, contrast, focus and clipping alongside regression tests.
