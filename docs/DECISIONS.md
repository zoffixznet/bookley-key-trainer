# Decisions

Running log of choices made where the spec left room, plus notable deviations. One line
each; newest at the bottom of each section.

## Build / process

- No git remote is configured. Per the spec, we do NOT create one and do NOT push; all
  work stays local. The user will add a remote later.
- Git identity left as the box default (`Zoffix Znet <git@zoffix.com>`); never overridden.
- `--max-turns` is not a flag in `claude` v2.1.191 on this box, so book calls do not pass
  it. We instead disable all tools (`--tools ""`) so the model only writes prose and
  cannot stall on a permission prompt; a non-`success` result subtype is still treated as
  an incomplete chapter.
- Every book call passes `--setting-sources ""` so the user's global hooks/skills (this
  box injects a large SessionStart "superpowers" hook) do not pollute the run, the token
  budget, or the parsed output. The bundled novelist plugin is added explicitly with
  `--plugin-dir`, so disabling other sources does not lose our skill.

## Agent protocol

- The agent is asked to wrap its output in explicit markers
  (`===CHAPTER=== / ===BIBLE=== / ===END===`, and `===QUESTIONS===` for the single
  clarifying turn). This makes parsing deterministic regardless of prose content and lets
  us split chapter text from the continuity bible reliably. If markers are missing we fall
  back to treating the whole reply as chapter text.
- Chapter parsing prefers the `result` field of `--output-format json`. Stream-json is
  used only for the live "writing..." view (text_delta chunks). This keeps parsing simple
  and robust; the live view is best-effort.
- Continuity context per chapter = the persisted bible + the previous chapter's tail +
  `--resume <session>` from the book's own directory. The spec's "story so far in a file,
  referenced by path" variant is pointless here because every run passes `--tools ""`
  (the agent cannot read files); prompt sizes stay a few KB, far under the stdin cap.
- Full craft on every call (per updated spec, line 79/80/110/280): the COMPLETE novelist
  craft lives in the always-on system prompt (`--append-system-prompt`) AND in the bundled
  `SKILL.md`, and we invoke the skill deterministically with `/novelist:write-chapter ...`
  at the top of every prompt (chapter, rewrite, and the clarifying turn). We do not rely on
  description-based auto-surfacing, and we do not trim/defer/summarize craft to save tokens.
  Redundancy between prompt and skill is intended. Tokens are explicitly not a concern.

## Authentication (Connect Claude)

- The in-app flow PTY-drives `claude setup-token` (not `claude auth login`): probing
  v2.1.191 showed auth login has no localhost callback either (both are paste-code
  flows), and setup-token is the one that yields a long-lived token the app can own.
- The PTY is opened 500 columns wide so the authorize URL is never line-wrapped by the
  terminal; scraping is ANSI-stripped substring/URL matching with generous timeouts and
  explicit failure states, all tested against a fake PTY script.
- The captured token is stored 0600 at `<config>/claude-oauth-token` and passed to every
  child as CLAUDE_CODE_OAUTH_TOKEN (subscription auth). A CLI that is already signed in
  is detected via `claude auth status --json` and skips the flow entirely.
- If the `claude` binary is missing, Book mode shows a message pointing at the README's
  `make install-claude` (installing a system package cannot be done from inside the app
  without sudo); signing in never requires a terminal.

## Compliance

- Anthropic ToS: their SDK docs say third-party developers may not offer claude.ai
  login / subscription rate-limits in products distributed to other users without prior
  approval. Personal single-machine use is fine. Since end users are expected to use their
  own subscription via the already-installed, already-authenticated `claude` CLI, this is
  recorded here and in the README (Known limitations) as a compliance consideration for the
  user to weigh before distributing. We proceed; it is the user's call.
- We never set `ANTHROPIC_API_KEY` in the spawned child env (sanitized out) and never use
  `--bare`, keeping generation on the subscription. Verified via `apiKeySource:"none"` in
  a real init event.
- Re-checked support.claude.com/en/articles/15036540 on 2026-07-10: the planned move of
  Agent SDK / `claude -p` usage to separate credits is paused; subscription draw remains
  in effect, as this app assumes.
- One real `make live-book-smoke` run was performed on 2026-07-10 (Opus) to verify the
  live path end to end; it passed and consumed a small amount of subscription usage. All
  other tests use the fake CLI.
- A second live run followed the third review batch (same day, Opus): full-size chapter
  plus a real cover design; the SVG parsed, validated, and rasterized on the first
  attempt with no fallback. Everything else runs against the fake CLI.

## Fonts / assets

- Fonts (superseded, see the user-review batch below): the first build shipped no font
  files and used egui's bundled fonts. The visual redesign now embeds IBM Plex Sans,
  IBM Plex Mono, and Young Serif (all OFL, ~450KB total, licenses in NOTICE).

## UI / behavior

- Stop-on-word is distinct from stop-on-letter: wrong letters advance within the word,
  but the word boundary (space/newline) blocks, even on a correct press, until the word
  is fixed with backspace. The blocked boundary press is counted as an error keystroke.
- The one clarifying turn is allowed for any chapter when the user gave direction; a
  confirmed-blank generation forbids asking, per the spec. The answer turn always
  disables further questions.
- For 100%-AI books (blank title), chapter 1's prompt asks the author to emit
  `BOOK-TITLE: ...` at the top of the bible; the app adopts it as the book title.
- Exports are written to `<data>/exports/<slug>.{md,pdf}` and the app shows the exact
  path; no native save dialog (keeps dependencies down, and the location is stable).
- Resuming a partially typed chapter restarts that chapter (superseded in the third
  review batch: typing progress now persists per chapter and resumes with a
  one-paragraph rewind; see below).
- No audio subsystem shipped in the first build (superseded: the key sound landed later;
  see the "Key sound" section below).
- Space is matched via egui Text events only (it produces one); Enter/Tab via key events
  (they do not). Matching both would double-count Space.
- Dev shortcuts: F9 auto-type (repeat-friendly), F10 completes a ~200-item "page", F12
  completes the whole target. F9/F10/F12 are excluded from the Random pool.
- Hidden `--screenshot PATH [--screen NAME]` flags boot the real app, render frames, and
  save a PNG; used to verify X11 rendering from scripts without a human watching.

## User-review batch (2026-07-10)

- Single-word mode became a timed multi-word drill (a flowing stream that refills as you
  type); Random keys uses the same drill timer since it was cheap and consistent. The
  duration presets are 30s/1m/2m/3m/5m, default 2 minutes. The old "keys per round"
  setting is gone from the UI (the timer governs); the config field remains for
  compatibility.
- Pause: a PauseClock subtracts paused gaps from the injected wall clock, so WPM and
  consistency are computed over active time only (unit test asserts a 60s pause changes
  nothing). While paused, input is ignored and the target is veiled so it cannot be read
  ahead.
- Typing-target normalization: implemented with an explicit punctuation/space map plus
  NFKD decomposition (unicode-normalization crate), dropping combining marks and
  anything else non-ASCII. deunicode was considered but rejected: it maps symbols to
  spelled-out words ((TM), etc.), which is wrong for a typing target. Disk files and
  exports keep original Unicode; only the displayed/compared target is normalized.
- The drawn keyboard is a full-size 104-key board (F-row grouping, real key widths, both
  Shifts, Caps, modifier row, nav cluster, inverted-T arrows, numpad with tall/wide
  keys). The numpad is shown by default (a full-size board is the point) with a Settings
  toggle. Modifiers, numpad, and the PrtSc cluster are drawn for realism but are not
  typing targets (egui reports modifiers as state, not keypresses, and has no numpad key
  identities); Guide mode rings Shift alongside the base key for shifted targets.
- Exports open with xdg-open, spawned detached and reaped on a background thread; a
  failed open degrades to showing the path only.
- The last-chapter checkbox injects an explicit conclude directive (replacing the normal
  arc guidance) and marks the book concluded on success; concluded books show a finished
  state instead of the generate block, and rewrites stay available.
- Fonts are now embedded (IBM Plex Sans, IBM Plex Mono, Young Serif, all OFL, recorded
  in NOTICE), superseding the earlier fonts-optional decision: the visual redesign
  needed a real serif/mono identity. The light foolscap theme is the default for fresh
  configs; a saved choice wins.
- Results: Enter triggers the primary action, guarded for 400ms after completion so the
  finishing keystroke cannot immediately restart; book completions show "Continue the
  book" instead of "Type again".

## Third review batch (2026-07-10)

- Chapter length: the 400-900-word target is gone from the system prompt, the bundled
  SKILL.md, and the spec (INITIAL_DESIGN updated to match). Chapters are now written as
  full printed-novel chapters, as long as the scene work demands, with an explicit "a
  three-page chapter is not a novel chapter" floor and no padding. The old reason for
  short chapters (bounded typing sessions) is handled by progress persistence instead.
- Book typing progress persists per chapter (position in the normalized target), saved
  on every completed paragraph, every ~3s, on mode switches/session restarts, and on
  exit. Resume rewinds to the greatest paragraph boundary strictly BEFORE the saved
  position (so stopping exactly on a boundary rewinds one full paragraph); the rewound
  paragraph is retyped as a refresher. Saves are monotonic: refresher retyping never
  regresses the saved position.
- Per-key stats: every keystroke books against the key that was EXPECTED at that moment
  (both char and physical paths), key auto-repeat is ignored (repeat Key events are
  dropped along with the Text event each produces; egui emits Key before its Text), the
  first keystroke contributes no latency sample, and keys without samples show a dash,
  never 0ms. Latency = inter-keystroke interval ending in a correct press, on the
  pause-adjusted clock.
- Backspace is a drillable target in Random mode only (it stays a correction key in the
  char modes, where the text could not be typed without it).
- Every mode starts behind a "Press Space to start" gate; the gate press is consumed.
  Space also resumes from pause. "Reset stats" (Paste/Book) zeroes metrics and re-arms
  the gate while keeping the typing position. Screenshot mode auto-starts unless the
  hidden --gate flag asks for the gate itself.
- Exports are book content only: title page + chapters (+ the cover as PDF page one).
  Premise/language/continuation stay in book.toml for the AI. The Markdown export stays
  text-only (a .md cannot self-contain a raster cover); the PDF carries the cover.
- Covers: claude designs a hard-constrained self-contained SVG (fixed 1600x2560 viewBox,
  basic shapes/paths/text/gradients, generic font families, no external refs/filters/
  scripts/fonts), validated defensively and rasterized by the existing resvg pipeline
  with the app's embedded fonts mapped to the generic families. One retry with the error
  fed back, then a local typographic fallback, so the button always yields a cover.
  Cover runs use a fresh claude session (not resumed into the novel's session) so design
  chatter never pollutes story continuity. Blank renders count as failures.
- Colorblind rule baked into the theme: state is never hue-only. Errors pair a brighter
  error ink with a background tint and underline (verified in grayscale); the results
  chart pairs color with dash pattern/weight plus a legend; weak-key pills are neutral
  chips carried by text.

## Content / metrics

- WPM uses the Monkeytype/Wikipedia 5-char convention. Consistency is the coefficient of
  variation of instantaneous WPM samples (one sample per correct keystroke), inverted and
  mapped to 0-100, per the Monkeytype definition.
- Random-keys mode draws from the full physical keyboard including non-character keys
  (arrows, function keys, nav cluster, Tab/Enter/Esc). Dev keys (F9/F10/F12) are excluded
  from the Random pool so they never clash with dev shortcuts.

## Key sound

- The typewriter click uses five real CC0 (public-domain) recordings bundled under
  assets/sounds/ and embedded in the binary: three distinct single-key strokes
  (Freesound, yottasounds) for ordinary keys and two deeper Hermes Precisa 305 thunks
  (BigSoundBank, Joseph Sardin) for Space/Enter/Backspace, each played with a few
  percent of per-press pitch/volume variation. Sources and licenses are in NOTICE and in
  the sound.rs module doc. rodio carries playback + wav decoding only; decoding happens
  once at init. The original procedural synth stays in the binary as an automatic
  fallback if a bundled sample ever fails to decode (one log line, no crash).
- The audio device opens lazily on the first click. If it fails (headless run, no
  device), sound latches off after one log line and the app carries on silently, so the
  smoke test and CI need no audio stack at runtime. Building does require the ALSA dev
  headers (libasound2-dev), now checked by make deps.
- The config field was renamed sound -> key_sound (serde default true) so configs saved
  before the feature, including ones carrying the never-surfaced sound=false stub, come
  up with the click on by default. The top-bar switch is session-only; Settings holds the
  launch default.

## Round-4 review and bug sweep

- An adversarial review plus a five-area bug hunt (two-skeptic verification per finding)
  ran over the whole codebase; 14 confirmed bugs were fixed. Highlights: all book-store
  writes (book.toml, bible, chapters, cover) are tmp+fsync+rename atomic so a torn write
  cannot orphan a book, and typed-progress save errors surface in the UI; the Connect
  Claude PTY scrape no longer truncates tokens/URLs split across read boundaries; cancel
  kills a silent generation child within one watchdog tick; a mid-stream read error can
  no longer forge a successful generation; timed drills auto-pause off the typing screen
  and results are labeled with the session's own mode; all content-mode switches go
  through one transition path; rewriting or deleting a book invalidates any live typing
  session on it; stale keyboard flashes cannot survive a clock reset.
- ASCII normalization guard: if folding would delete most of the letters (non-Latin
  scripts such as Russian), the original characters are kept as the typing target
  instead of collapsing the chapter to punctuation; smart punctuation and whitespace
  rules still apply.
- PDF export includes system fonts (embedded fonts remain the deterministic base) and
  sets the Typst text language from the book's language field for hyphenation.
- The hover label shift was egui 0.35's selectable-button frame sizing; every
  interactive widget state now uses equal stroke widths with expansion equal to the
  stroke width (unit-tested), so labels never move on hover. Side effect: widgets no
  longer grow a pixel on hover anywhere.
- Screenshot mode gained a hidden BOOKLEY_SHOT_DELAY_MS env var so external drivers
  (xdotool) can position the real pointer before capture, making hover states and
  tooltips screenshot-verifiable.
- Book jacket redesign: the open book renders as cover (or a typographic placeholder)
  beside title/status/progress with one grouped toolbar, a chapters table where only the
  first untyped chapter gets a Type button (the session always serves that chapter), and
  exactly one prominent next-step block. The app remembers the last open book
  (config: last_book); Book-mode launches reopen it straight onto the typing stage with
  the Space gate armed.

## Install and round-5 fixes

- make install puts a single self-contained binary at ~/.local/bin/bookley plus the
  desktop entry and hicolor icons, removing the previous copies of exactly those files
  first; make uninstall removes the same fixed list and nothing else. Everything the app
  needs at runtime is embedded (the novelist plugin re-stages itself into the data dir
  on every launch, so upgrades refresh it). User data (books, settings, stats) is never
  referenced by either target; books are sacred.
- Start-gate overlay bug root cause: the scrim and the card were two Areas in the same
  egui order class, and egui raises a clicked Area to the top of its class and persists
  that z-order in Memory for the run, so one click on the scrim buried the card (and its
  duration buttons) until restart, across mode switches. Fixed by putting the scrim in
  Order::Middle and the card in Order::Foreground: different classes can never be
  reordered against each other.
- Open-book shelf tile: the badge and thicker border/spine changed tile geometry,
  re-wrapped long titles, and misaligned the grid. The active cue is now a slightly
  darker tile background plus the disabled "Opened" button, with identical geometry on
  every tile.

## Interactive hunt (round 6)

- The app was driven end to end with real clicks and keystrokes (xdotool) on an
  isolated Xvfb display, with a frame capture inspected after every step: gate-overlay
  scrim clicks, the on-card duration picker, pause/resume with noise input while
  paused, mid-drill mode switches, the paste flow, reset stats, results tooltips,
  Enter-retype, book open/export/rewrite/type, kill-and-restart resume (log-verified:
  "resuming ... at 72 of 3416 (rewound from 110)"), live theme switch, numpad toggle,
  and window resizing.
- Two real bugs found and fixed: (1) dev shortcuts (F9/F10/F12) recorded absurd
  results into the real stats and personal bests (a 2871-wpm paste "PB"); any session
  touched by a dev shortcut is now tainted, its results screen says "dev-assisted run:
  not recorded", and nothing is written to stats or PBs (unit-tested). (2) At narrow
  window widths the top bar's right cluster plowed over the mode tabs into an
  unreadable mash; the bar now drops its SOUND/KEYBOARD eyebrow labels below ~1235px
  and the window minimum inner size is 1120x640. The minimum is enforced by the window
  manager; a WM-less X server (bare Xvfb) can still force smaller, which is a test-rig
  artifact, not a desktop behavior.
- Verified clean in the same session: the scrim/overlay z-order fix (repeated scrim
  clicks never bury the card), exports (viewer opens; content is title + chapters
  only), and the whole reopen-last-book flow (launch lands on the typing stage of the
  next chapter with the gate armed).

## Naming (round 7)

- Everything the app puts on a user's machine now uses the full name
  bookley-key-trainer: the installed binary, the desktop entry (Exec/Icon/WMClass), the
  hicolor icons, the app id, and the XDG config/data dirs
  (~/.config/bookley-key-trainer, ~/.local/share/bookley-key-trainer). Legacy
  "bookleykeytrainer" dirs are renamed automatically at launch (same-filesystem rename;
  books move with it; on failure the legacy dir is left untouched). make install and
  make uninstall also remove pre-rename bookley-named binary/desktop/icons so upgrades
  never leave stale copies. The Cargo package/lib name stays "bookley"; only the bin
  target is renamed.

## Bootstrap (round 8)

- make deps became an installer instead of a checker: on apt systems it installs only
  what is missing (build-essential, pkg-config, curl, libasound2-dev, libwayland-dev,
  libxkbcommon-dev, libx11-dev) with sudo, and installs rustup when there is no Rust;
  on a machine that has everything it does nothing and never asks for sudo. Non-apt
  systems get the package list printed. The Makefile resolves cargo from
  ~/.cargo/bin as a fallback so make run works in the same shell that just ran deps.
  make run / make install intentionally do NOT depend on deps (documented in the
  Quick start, along with the fact that a fresh Ubuntu lacks make itself and needs
  sudo apt install make git first).

## Round 9 (Wayland/KDE feedback)

- The paste box gained a right-click menu (Paste / Paste, replacing / Clear) backed by
  arboard, since egui only surfaces clipboard content on Ctrl+V events; the box keeps a
  fixed height with an inner always-visible scrollbar and the whole screen page-scrolls,
  so a huge paste can never push the Start button out of reach.
- Action buttons are laid out with add_sized via a shared theme::action_button helper:
  min_size alone left labels left-aligned with lopsided padding on some backends
  (reported on Wayland).
- "Upload cover" uses rfd's xdg-portal backend (KDE/GNOME native dialogs, no GTK) on a
  background thread; the image (PNG/JPEG/WebP, <=50 MB) is bounded to the cover canvas,
  re-encoded as PNG, and stored exactly like a generated cover. A canceled dialog just
  stands down.
- The installed desktop entry's Exec/TryExec are rewritten to the absolute binary path:
  KDE and friends resolve Exec against the session PATH, which often lacks ~/.local/bin
  even when interactive shells have it, which made launcher/taskbar launches fail with
  "could not find the program".
