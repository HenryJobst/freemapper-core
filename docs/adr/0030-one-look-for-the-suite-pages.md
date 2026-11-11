# 0030. One look and one frame for the suite's local pages

Date: 2026-11-11
Status: Accepted (open: who converts freemapper's area page, and when)

## Context
The suite has three browser pages on the local server of ADR 0020:
- freemapper's area page: plain JS, texts in `TEXTS.de/en` in `app.js`, a fixed `app.css` without variables or a
  dark mode, Leaflet vendored.
- freemapper-courseplan's editor (its ADRs 0042 to 0046): CSS tokens with a dark mode, an `api.js` wrapper,
  a revision header, an undo stack, Fluent texts through `api/texts`, a tree with keyboard rules.
- freemapper-print's editor: planned (print ADR 0005, plan `2026-11-11-m6-browser-editor.md`).

The user decided on 2026-11-11 that all three look and work alike, and left the target design to the sessions.
freemapper coordinates as owner of `freemapper-web`. courseplan, print and FableWorker (owner of the area page)
agreed to the points below; their conditions are part of the decision.

## Decision
- **Reference** *(agreed by all)*. The courseplan editor is the reference, because it is the furthest along.
- **Where the shared part lives** *(agreed by all)*. `freemapper-web` embeds a small set of suite assets and
  serves them below the page token at `/{token}/suite/<file>`, next to the app's own routes. The core tag pins them
  (freemapper ADR 0067). Nothing comes from the network. The shared part holds only frame and patterns:
  - `suite.css`: the tokens, light and dark (`prefers-color-scheme`), base elements (buttons, inputs, dialogs,
    notices), the menu bar and the status bar, a visible focus ring, no animation under
    `prefers-reduced-motion`.
  - `api.js`: `get`, `post`, `postBytes`, `getFile`, each answering `{ ok, status, data }`; a POST sends a
    non-simple content type and, when given, the revision header.
  - `texts.js`: loads `api/texts`, applies `data-text`, `data-label`, `data-title`, `data-placeholder`, sets
    `document.documentElement.lang`, and offers `t(id, args)` and `tn(id, n, args)` (see Texts).
  - `status.js` and `tree.js`: the status bar and the tree (`role="tree"`, arrow keys, Home, End, Enter, one
    tab stop), taken from courseplan. A page uses the modules it needs.
  - On the Rust side: `revision(bytes) -> String` and the 409/422 answers below, and a test helper that checks a
    page's assets against the rules under Security and size.
  Everything about the subject stays in the apps: maps, overlay, tools, key bindings for courses, tiles (freemapper
  ADR 0088), the run controls and the replace dialog (freemapper ADR 0090). The CSP stays a parameter of the app
  (ADR 0020), so freemapper keeps its tile and aerial-photo hosts.
- **Tokens** *(courseplan's names; colour by freemapper)*. The names are courseplan's: `--bg --fg --line --bar
  --accent --sel --map-bg --pool --warn-bg --warn-fg --warn-line --sev-error --sev-warning --sev-hint`. They stay
  stable; apps change values only through new core tags. The suite accent is freemapper's blue: `--accent` and
  `--sel` are `#0b6bcb` in light mode and `#6cb0ff` in dark mode. courseplan's purple goes, because on a map page a
  purple accent reads like the course overprint. App-specific tokens stay in the app with the app's prefix-free
  name, as `--course` (courseplan's magenta overprint, `#b0007a` / `#ff5cc8`).
- **Texts** *(Fluent as in courseplan ADR 0022/0045; the language rule as in freemapper ADR 0092)*.
  - Every visible word comes from the app's Fluent catalog through `GET api/texts`, which answers
    `{ lang, texts }`. Page messages keep the app's id family (`editor-*` in courseplan, a `page-*` family in
    freemapper). The words stay as they are; AGENTS.md and the manuals quote them.
  - Language: `--lang`, then the app's language variable, then the browser's language from `Accept-Language`
    (German or English), then English. The page has no language switch; the server decides once per request of
    `api/texts`. This keeps freemapper ADR 0092 and changes courseplan only when nothing is set.
  - Values from the server come formatted by the server, as today in courseplan.
  - Values the page itself knows while the person draws (size, tile count, mode hints) must not cost a round trip.
    For them `api/texts` also delivers patterns with simple placeholders `{ $name }`, no selectors and no
    functions; `t(id, { name: value })` fills them in. A count picks between two messages `<id>-one` and
    `<id>-other` with `Intl.PluralRules(lang)` through `tn(id, n, args)`. Numbers in such texts are formatted with
    `toLocaleString(lang)`.
- **Changes and revisions** *(courseplan ADR 0043/0044; the area page as justified exception)*.
  - The revision of a file is FNV-1a-64 of its bytes in 16 lower-case hex digits, sent in `X-Freemapper-Revision`.
    A write whose revision no longer matches gets 409 with the current revision; a change the app refuses gets
    422 with one sentence for the person. A page that switched to read-only answers 423 itself, without a request.
  - Editors (courseplan, print) write each change at once and keep an undo stack of whole-file snapshots.
  - freemapper's area page is no editor. It keeps its save button and its dialogs: saving an area can drop fetched
    data, and creating the map can replace a map with the mapper's work in it, which no undo brings back. It takes
    over the look, `api.js`, `texts.js` and the revision check on its save route, so two open windows cannot
    overwrite each other.
- **Security and size** *(courseplan's rules, now suite rules)*. Checked by the core test helper in every app:
  no inline script, no inline handler, no script, stylesheet or font from the network (images only from the hosts
  the app's CSP names, as freemapper's aerial photos), every POST with an Origin check, a
  non-simple content type and, where it changes a file, the revision header. The browser never shows a file path
  it did not get from the person. The app's own assets plus the suite assets stay below 300 KiB per program;
  vendored third-party libraries under `web/vendor/` (Leaflet in freemapper) count separately, with their own limit
  in the app.

## Work split
1. Core (freemapper, owner of `freemapper-web`): move `suite.css`, `api.js`, `texts.js`, `status.js`, `tree.js`
   from courseplan into `freemapper-web`, add `revision`, the 409/422 helpers and the test helper, release a core
   tag. courseplan reviews the moved files.
2. courseplan: switch to the core assets and `X-Freemapper-Revision`, take the suite accent, read the language as
   above. Its unmerged branch builds against its current state and is ported afterwards.
3. print: builds its editor on the core assets from the start.
4. freemapper's area page: texts to Fluent (`page-*`, about 145 messages from `app.js` and `run.js`), the core
   assets, tokens with a dark mode, the revision header on save, the browser smoke test. Open: done by FableWorker
   after merge 1 of the adapter refactor (freemapper ADR 0082), or by the freemapper self-update session sooner,
   either way before or after the adapter refactor's step C13, not in between. The user decides.

## Consequences
- The three pages look alike, switch to dark mode together, and share one way to call the server, show texts and
  guard writes. A fix in the frame reaches all three with the next core tag.
- courseplan loses its purple accent and renames its revision header.
- The area page gets a dark mode and Fluent texts. Its save button stays, as a documented exception.
- Changing a token name or the meaning of 409/422 becomes a core change that all three apps take together.
