# Bookley Key Trainer — implementation plan

Working plan derived from `docs/INITIAL_DESIGN.md` after a verification pass against
live sources on this machine (2026-07-10). This is the build order and the decisions
that shape the code. Deviations and ambiguities get a line in `docs/DECISIONS.md`.

## Research findings (verified on this box)

Toolchain: rustc/cargo 1.97.0. Session is X11 (`DISPLAY=:0`, no `WAYLAND_DISPLAY`).
`claude` is `/usr/bin/claude` v2.1.191, logged in via `claude.ai` subscription (max plan).

`claude --help` confirmed the flags we need:
`-p/--print`, `--output-format {text,json,stream-json}`, `--verbose`,
`--include-partial-messages`, `--system-prompt`, `--append-system-prompt`,
`--model` (aliases `opus`/`sonnet`/`haiku`/`fable`), `--max-turns` is NOT present in
this build (the turn cap is not a flag here); `--session-id <uuid>`, `-r/--resume`,
`-c/--continue`, `--fork-session`, `--setting-sources`, `--plugin-dir <path>`,
`--tools` (use `--tools ""` to disable all tools), `--permission-mode` (has
`dontAsk`). `claude auth status` prints JSON with `loggedIn`, `authMethod`,
`subscriptionType`.

Note: `--max-turns` is absent in v2.1.191, so we do not pass it. We disable all tools
with `--tools ""` so the model only writes prose and cannot block on a permission
prompt; that removes the main reason to cap turns. We still classify a non-success
result subtype as an incomplete chapter.

Real probe of `--output-format json` (trivial prompt) returned:
`{"type":"result","subtype":"success","is_error":false,"result":"pong",
"session_id":"...","total_cost_usd":...,"num_turns":1,"usage":{...}}`.

Real probe of `--output-format stream-json --verbose --setting-sources "" --plugin-dir <dir>`
returned one line per event:
- `{"type":"system","subtype":"init", model, tools, plugins:[{name,path,source}],
  plugin_errors(absent=none), mcp_servers, slash_commands, session_id,
  apiKeySource:"none", cwd}` — `apiKeySource:"none"` confirms subscription (no API key).
  Our throwaway plugin loaded and `novelist:write-chapter` appeared in `slash_commands`
  with no `plugin_errors`.
- `assistant` events carry `message.content` blocks (`thinking`, `text`).
- final `{"type":"result","subtype":"success","result":"<text>","session_id":...}`.

With `--include-partial-messages`, `stream_event` events appear:
`event.type=="content_block_delta"` with `event.delta.type=="text_delta"` carrying
`event.delta.text` (the incremental chunk). This is our live "writing..." feed.

We pass `--setting-sources ""` on every book call so the user's global hooks/skills
(this box injects a large SessionStart hook) do not pollute the run or the output.

Crate versions resolved via crates.io: eframe/egui 0.35.0, pulldown-cmark 0.13.4,
typst-as-lib 0.16.0, directories 6.0.0, clap 4.6.1, serde 1, toml 1.1, serde_json 1,
rand 0.10.2, resvg/usvg 0.47.0, tracing 0.1, tracing-subscriber 0.3. Pins recorded in
Cargo.toml; the committed Cargo.lock is the real record.

## Architecture

Single binary `bookley` with a library-style module split so the core is unit-testable
without egui:

- `core/` (no egui):
  - `metrics.rs` — WPM (5-char), raw WPM, accuracy, consistency (CoV of instantaneous
    WPM samples), per-key stats, WPM-over-time samples.
  - `keys.rs` — the `Key` model (logical + physical), finger-zone table, keyboard layout
    rows, dev-key set, display names for non-character keys.
  - `session.rs` — the typing session state machine: expected sequence, cursor, error
    handling mode (off/letter/word), correct/incorrect tracking, dev auto-type hooks.
  - `text_source.rs` — `TextSource` trait + Random, Word, Paste, Book implementations
    that yield a target string / sequence of expected keys.
  - `wordlist.rs` — bundled EFF long wordlist via `include_str!`, provider.
  - `config.rs` — settings struct, TOML load/save under `directories` config_dir.
  - `stats_store.rs` — personal-best + history persistence under data_dir.
  - `book/` — book store (Markdown + toml + bible on disk), agent client (spawns
    `claude` behind a `CommandRunner` trait), prompt builder, event parser, PDF export
    (typst-as-lib), Markdown export (pulldown-cmark for typed-text extraction).
- `ui/` (egui): app shell, top bar, typing stage, keyboard widget, results overlay,
  books manager, settings, theme/palette/fonts.
- `main.rs` — clap parsing (hidden `--dev`, `--smoke`), logging init, run modes
  (normal GUI, `--smoke` headless self-test).

Agent client uses a `CommandRunner` trait so tests inject a fake. Real runs spawn
`claude` on a background thread; events flow to the UI over an `mpsc` channel. The
child env is sanitized: `ANTHROPIC_API_KEY` removed; never `--bare`.

Binary path override: env `BOOKLEY_CLAUDE_BIN` (default `claude`).

## Book on disk

`data_dir/books/<slug>/`:
- `book.toml` — title, language, premise, created, session_id, chapters:[{n, file,
  typed_chars, done}].
- `bible.md` — continuity bible (agent maintains; we parse it out of the reply).
- `chapters/NN.md` — each chapter Markdown.

Generation reply format: the agent returns a fenced/delimited structure we can split
into chapter prose + bible. We instruct it (in the skill) to emit
`===CHAPTER===` ... `===BIBLE===` ... `===END===` markers so parsing is deterministic
regardless of prose content. Clarifying-turn replies use `===QUESTIONS===` marker.

## Bundled skill/plugin

`assets/plugin/novelist/` with `.claude-plugin/plugin.json` and
`skills/write-chapter/SKILL.md` carrying the heavy craft (bible schema, forbidden-tics
list citing arXiv 2409.14509, scene-vs-summary, pacing/hook checklist, output markers).
The lean system prompt goes via `--append-system-prompt`. `--plugin-dir` points at this
dir on every book call. Bundled path resolved relative to the binary / repo assets and
copied into data_dir on first use so it works from an installed location.

## Icon

`assets/icon/bookley.svg` (a keycap that reads as an open book, brass/verdigris/ink),
rendered to PNG 16..256 by `build.rs`-adjacent script `assets/icon/gen_icons.rs` run via
`make icons` (a tiny standalone cargo bin using resvg/usvg). `.desktop` file +
hicolor layout under `assets/`. Window icon set at runtime from the 256 PNG bytes.

## Testing

- Core unit tests: metrics (known sequences, all-errors, 5-char), text sources
  (pool honored, dev keys excluded, paste exact), book store round-trip, agent parser
  fed recorded stream-json/json (fixtures) asserting chapter text/session_id and
  classifying error_max_turns/rate_limit/logged-out, PDF `%PDF` + length.
- Fake `claude`: `tests/fake_claude.sh` emits canned stream-json; used by `make smoke`.
- `--smoke`: headless self-test that runs a scripted dev auto-type session through the
  real core pipeline, plus a fake-claude book generate/persist/export cycle, asserts
  stats via exit code + assertable log lines.
- `make live-book-smoke`: opt-in, real `claude`, one tiny chapter.

## Implementation order

1. Scaffold: Cargo project, deps, Makefile, docs, git init + first commit.
2. Core: keys, metrics, session, text sources, wordlist, config, stats. Unit tests.
3. Book: store, prompt builder, event parser, agent client (fake-backed), exports.
   Unit tests + fixtures.
4. UI: theme/fonts, app shell, typing stage, keyboard widget, results, settings,
   books manager. Wire dev mode + badge.
5. Icon: SVG + generator + desktop wiring + runtime window icon.
6. Smoke: `--smoke`, fake claude, live-book-smoke target.
7. README + final DoD pass, full suite green, final commit.
