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
- Resuming a partially typed chapter restarts that chapter (per-chapter done state is
  what gates generation); chapters are 400-900 words, so the cost is minutes.
- No audio subsystem shipped, so there is no sound toggle in Settings (a dead toggle is
  worse than none); the config field remains and persists for forward compatibility.
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

## Content / metrics

- WPM uses the Monkeytype/Wikipedia 5-char convention. Consistency is the coefficient of
  variation of instantaneous WPM samples (one sample per correct keystroke), inverted and
  mapped to 0-100, per the Monkeytype definition.
- Random-keys mode draws from the full physical keyboard including non-character keys
  (arrows, function keys, nav cluster, Tab/Enter/Esc). Dev keys (F9/F10/F12) are excluded
  from the Random pool so they never clash with dev shortcuts.
