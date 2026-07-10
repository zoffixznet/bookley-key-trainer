# Bookley Key Trainer — build spec

This document (`docs/INITIAL_DESIGN.md`) is the complete specification for building **Bookley Key Trainer**, a desktop typing-speed trainer written in Rust that runs on Linux under both X11 and Wayland. Read it end to end before writing any code, and come back to it whenever requirements get fuzzy. It is the single source of truth for this job.

---

## Working agreement (read first, overrides your defaults)

- Work **fully autonomously**. Do not ask the user questions. Do not present options and wait for a pick. Do not pause to ask whether to continue. Keep going until every item in the Definition of Done is met.
- A half-finished project handed back with a list of "next steps" is a **failure**. Finish it.
- When something is ambiguous, choose the most sensible option, write a one-line note in `docs/DECISIONS.md`, and keep moving.
- **Test everything yourself.** No human will be watching the screen or eyeballing output. Any check that would normally need a person gets a scriptable stand-in instead: assertable logs, a self-test/smoke mode, screenshots, a fake `claude` binary, mock data. If you cannot verify something in this environment, record it in the README under "Known limitations" — never turn it into a question.
- `docs/INITIAL_DESIGN.md` (this file) is the source of truth. Work from the document, not from a stale memory of it. Re-read it whenever things feel underspecified.
- Do **not** modify anything outside the project directory. Reading reference material and the local `claude` CLI is fine; changing system state is not.
- The one exception to "no questions" is truly missing credentials the build cannot proceed without, and even those are already handled: the git identity is configured and the `claude` CLI is installed and logged in. So there is nothing to stop for.

---

## What the app is

Bookley Key Trainer is a polished GUI typing trainer with a twist: one of its practice modes generates a **real novel, one chapter at a time, using the Claude Agent SDK** — so as the user practices typing, they are literally producing a book they can export to Markdown or PDF.

Two orthogonal axes of settings define a session:

1. **Keyboard display mode** — how the on-screen keyboard behaves.
2. **Content mode** — where the text-to-type comes from.

### Keyboard display modes (exactly three)

1. **Guide** — draw the on-screen keyboard and **highlight the next key to press**, so the user can glance down at the screen to locate it.
2. **Feedback** — draw the on-screen keyboard, give **no** next-key hint, and only **briefly flash the keys the user just physically pressed** (a short highlight that fades).
3. **Hidden** — do not draw the keyboard at all.

### Content modes (four)

1. **Random keys** — show random keys the user must press. Not just letters: include the whole keyboard so the user learns every key — letters, digits, punctuation, and non-character keys like arrows, Page Up / Page Down, Home / End, Insert / Delete, Tab, Enter, Esc, and the function keys. (The keyboard-display highlight must work for these physical keys too.)
2. **Single word** — show one word at a time drawn from a bundled word list; the user types it, then the next word appears.
3. **Paste text** — the user pastes an arbitrary chunk of text into a text box; the app makes them type exactly that text.
4. **Book** — AI-generated fiction. See the dedicated section below; this is the differentiating feature and the largest piece of work.

### Developer mode (hidden behind a CLI flag)

Launching with a hidden `--dev` flag enables developer shortcuts so you (and your tests) can complete typing content without actually typing it:

- One key **auto-registers the currently-expected key as correctly pressed**. Holding it (key auto-repeat) should keep registering the next expected key, so you can lean on one key and "type" a whole chapter.
- One key **instantly completes the current page/screen** of text (registers it all as correctly typed).
- One key **instantly completes the current chapter / whole current text** (registers it all as correctly typed).

Pick key bindings that will not collide with real typing content (function keys are a good default: for example F9 = auto-type next expected key, F10 = complete page, F12 = complete chapter). In Random-keys mode, exclude whatever dev keys you choose from the pool of requested keys so they never clash. Show a small "DEV" badge in the UI when this mode is on. The exact bindings are your call; the three behaviors are the requirement.

---

## Verified technical pointers (from research — re-verify each against the real source before trusting it)

These were gathered from research on 2026-07-10. Treat them as strong leads, not gospel: **re-verify each against the live source / `docs.rs` / `claude --help` before you rely on it.** Versions in particular may have advanced.

### GUI stack (this machine, checked)

- The box has Rust 1.97, cargo, rustup. GUI dev libs present: `libx11`, `libxcb`, `libxkbcommon`, `libwayland-client/cursor/egl`, `libegl`, `libgl`, Vulkan 1.3, fontconfig, freetype. Build tools (`cc`, `gcc`, `make`, `pkg-config`) present. `libgles2` and `libudev` dev packages are absent but are **not** needed by the recommended stack. **No `sudo` installs are required.** If you somehow hit a missing `-dev` package, record it in the README rather than installing it silently; but you should not need to.
- Current session is X11 (`DISPLAY=:0`, no `WAYLAND_DISPLAY`). You can fully test X11 here; Wayland must be assumed-correct via the same code path (the user tests Wayland on another box). Do not break Wayland to make X11 work.

### Rust GUI: eframe / egui

- Use `eframe`/`egui` **~0.35** (0.35.0 released 2026-06-25). Default Cargo features include **both `wayland` and `x11`** plus `wgpu` — so X11 + Wayland work out of the box; `winit` picks the protocol at runtime (override env historically `WINIT_UNIX_BACKEND=x11|wayland`). **Do not** set `default-features = false` without re-adding `wayland` and `x11`, or you lose a Linux backend. Verify on `crates.io/crates/eframe` and `docs.rs`.
- `egui::Event::Key { key: Key, physical_key: Option<Key>, pressed: bool, repeat: bool, modifiers: Modifiers }`. `key` is **logical** (respects the active keymap/Dvorak); `physical_key` is the **layout-independent physical position**. There is also `Event::Text(String)` for produced characters (note: Enter/Return produces no Text event), and `Event::Paste(String)` / `Event::Copy` / `Event::Cut`. Verify field names/order on `docs.rs/egui` `enum.Event`.
- **Highlight the on-screen keyboard by `physical_key`** (the physical position is exactly the intent for a trainer, even though egui's docs mildly discourage `physical_key` for general apps). Use `key` / `Event::Text` / `Event::Paste` for *what was typed*. `physical_key` is `Option` because some keys map to `None`; have a fallback.
- Filter `repeat == true` when you only want first-press (except in dev auto-type, where repeat is what you want).
- Custom keyboard drawing: `ui.allocate_painter(size, Sense)` / `allocate_exact_size`; paint with `Painter::rect_filled(rect, CornerRadius, fill)`, `Painter::rect_stroke(rect, CornerRadius, Stroke, StrokeKind)`, `Painter::text(pos, Align2, text, FontId, Color32)`. Note API churn: 0.35 uses **`CornerRadius`** (old tutorials say `Rounding`), `rect_stroke` takes a **`StrokeKind`** arg, and `FontData` must be wrapped in **`Arc`** for `FontDefinitions`. Fade a just-pressed key with `ctx.animate_bool_with_time(id, active, secs)` and request repaints while any key is animating.
- Embed a font: `FontData::from_static(include_bytes!("..."))` wrapped in `Arc`, inserted into `FontDefinitions`, applied via `ctx.set_fonts(...)`; tune `ctx.set_style(...)` for a non-default look.
- Renderer: `wgpu` is the default (Vulkan/GL via wgpu). Keep `glow` (OpenGL) as a documented fallback via the `glow` feature + `NativeOptions.renderer = Renderer::Glow` for machines where wgpu/Vulkan init misbehaves. Degrade gracefully if the GPU backend fails to initialize.

### Claude Agent SDK via the `claude` CLI (book generation)

- There is **no official Rust Agent SDK** (only Python `claude-agent-sdk` and TS `@anthropic-ai/claude-agent-sdk`). The documented first-party path for other languages is to **drive the installed `claude` CLI as a subprocess** — the CLI is the Agent SDK's CLI surface. This box has `/usr/bin/claude` v2.1.191, authenticated. Spawn it as a child process.
- **Subscription billing (the whole reason for using the Agent SDK):** `claude -p` / the Agent SDK currently draws from the user's Claude Pro/Max **subscription** usage when logged in via subscription (not per-token API billing). Anthropic *paused* a change that would have moved this to separate credits; "for now, nothing has changed." Re-read `https://support.claude.com/en/articles/15036540` near completion. Two hard rules follow:
  - **Never set `ANTHROPIC_API_KEY`** in the spawned child's environment (its mere presence can flip usage to API billing). Sanitize the child env.
  - **Never use `--bare`** — bare mode uses API-key/apiKeyHelper auth only and never reads the subscription OAuth/keychain, which would break subscription billing.
- Flags you will use (confirm with `claude --help`): `-p/--print` (headless), `--output-format text|json|stream-json`, `--verbose` (**required** for stream-json), `--include-partial-messages` (token deltas; print+stream-json only), `--system-prompt` / `--append-system-prompt` (and `-file` variants), `--model` (aliases include `opus`, `sonnet`, `haiku`, `fable`), `--max-turns <n>`, `--session-id <uuid>` / `-r/--resume [id]` / `-c/--continue` / `--fork-session`, `--setting-sources`, `--plugin-dir <path>` (repeatable), `--allowedTools` / `--disallowedTools` / `--tools` (use `--tools ""` to disable all tools so it only writes prose and never blocks on a permission prompt), `--permission-mode` (e.g. `dontAsk`).
- **Default generation model = Opus (`--model opus`).** Book mode defaults to Opus for novel generation, and exposes a **model picker in the Settings screen** (at least `opus`, `sonnet`, `haiku`, `fable`) so the user can change it. Verify the alias resolves with a trivial `claude -p 'hi' --model opus`.
- **Preloading expert writing skill (the user asked): yes, this works.** A skill is plain Markdown: a plugin directory with `.claude-plugin/plugin.json` (`{"name":"novelist", ...}`) and `skills/<skill-name>/SKILL.md` carrying YAML frontmatter (`name`, `description`, optionally `allowed-tools`, `model`, etc.) followed by craft instructions. Load it per run with `--plugin-dir <dir>`; **invoke it explicitly and deterministically** via `/<plugin>:<skill>` in the prompt on every book call (e.g. `/novelist:write-chapter ...`). Do **not** rely on `description` auto-surfacing to decide when it applies. Bundle this skill dir with the app and pass `--plugin-dir` on every book call. Verify by loading the dir and checking the `system`/`init` event lists it with empty `plugin_errors`.
- **The full novelist craft must apply to every generation, unconditionally.** Put the complete craft into the always-on system prompt (via `--append-system-prompt` / `--system-prompt`) **and** deterministically invoke the bundled skill, so the same full guidance governs every chapter, every rewrite, and the clarifying turn. It is fine (intended, even) for the craft to be present in both the system prompt and the skill. Never leave "is the writing skill applied this time?" to chance or to description-based auto-surfacing. This is about guaranteeing quality and consistency, not about inflating prompt size: keep the craft text **tight and high-signal**, do not pad it or add filler to make it "thorough." Full coverage, no bloat.
- **Result JSON shape** (`--output-format json`): `{ type:"result", subtype:"success"|"error_max_turns"|"error_during_execution", is_error, result, session_id, total_cost_usd, num_turns, duration_ms, usage:{...} }`. The generated chapter text is in `.result`. For **stream-json**, each line is one JSON event: a first `system`/`init` (carries `session_id`, model, tools, plugins, `plugin_errors`), incremental `stream_event`s where `event.delta.type == "text_delta"` carry `event.delta.text` (append these for a live "writing…" view), possible `system`/`api_retry` events, and a final `result` event. Capture one real run and diff the actual keys before hardcoding.
- **Failure classification:** `api_retry` events carry an `error` category enum: `authentication_failed`, `oauth_org_not_allowed`, `billing_error`, `rate_limit`, `overloaded`, `invalid_request`, `model_not_found`, `server_error`, `max_output_tokens`, `unknown` (plus `attempt`, `max_retries`, `retry_delay_ms`). Map these to friendly UI states. Also: `--max-turns` exhaustion exits with `subtype: error_max_turns` and non-zero status — treat as an *incomplete* chapter, not a clean stop. Piped stdin is capped ~10MB; write long "story so far" context to a file and reference its path rather than piping the whole novel.
- **Multi-turn continuity across chapters:** capture `session_id` from chapter 1's result, then `--resume <id>` for later chapters **from the same working directory** (session lookup is cwd-scoped). Use `--fork-session` to try alternate drafts (rewrites) without mutating the original thread. Even so, do not rely solely on the session; also persist the full book to disk (below) and pass an explicit "story so far" / continuity note into each prompt so generation is reproducible and survives session loss.
- **In-app authentication (hard requirement — the user must never need a terminal):** assume the end user is already logged into claude.ai in their browser and is NOT proficient with CLIs. The app owns the entire authentication flow through its own UI; it never instructs the user to run a command. Build a **"Connect Claude"** flow:
  - **Default mechanism — PTY-drive `claude setup-token`:** spawn Anthropic's own login flow under a pseudo-terminal (e.g. the `portable-pty` crate), scrape the OAuth authorize URL it prints, and render it in-app as a clickable link (plus a copy button). The link opens in the user's logged-in claude.ai browser session; they click Authorize; the page displays a short authentication code; they paste it into a field **in the app**, which forwards it to the PTY. Capture the resulting long-lived OAuth token (`sk-ant-oat…`) from the CLI output, store it in the app's config dir with mode 0600, and pass it to every spawned `claude` child as **`CLAUDE_CODE_OAUTH_TOKEN`** (this preserves subscription billing; it is NOT `ANTHROPIC_API_KEY`, which must never be set).
  - **Smoother variant, probe it first — PTY-drive `claude auth login`:** the CLI opens the browser itself and completes via a localhost callback with **no code paste at all**; the app watches for completion and shows the scraped URL as a clickable fallback link in case no browser opens. If this proves reliable on the installed CLI, prefer it and keep the setup-token path as the robust fallback.
  - If the CLI is already authenticated (e.g. a dev box), detect it (`claude auth status`, or a trivial probe run) and skip the connect flow entirely.
  - Treat CLI output as an **unstable interface**: check `claude --version` at startup, scrape defensively (regexes for the URL/code-prompt/token, generous timeouts, explicit failure states), and cover the whole connect flow with tests against a **fake PTY script** so no test requires real auth. A last-resort "manual setup" screen may show terminal instructions, but it is a last resort, not the flow.
  - Headless generation runs (`-p`) must never turn interactive: close/ignore stdin, run children off the UI thread with a timeout/cancel path so a blocked CLI can never freeze the app. If a run fails with `authentication_failed` / `oauth_org_not_allowed` (or an auth-shaped non-zero exit), stop, preserve all prior chapters, and reopen the Connect Claude flow with a retry.
  - Book mode is the only thing gated on auth; the Random/Word/Paste modes must keep working with no `claude` present at all.
  - Allowed alternative if PTY scraping proves untenable: implement the same OAuth PKCE authorize/token exchange natively in Rust (it is the very flow the CLI performs) behind the identical "Connect Claude" UI. Research and verify the endpoints first, and record in `docs/DECISIONS.md` that it relies on undocumented endpoints.
- **ToS caveat to record (not a blocker):** Anthropic's SDK docs state that, unless previously approved, third-party developers may not offer claude.ai login / subscription rate-limits *in products distributed to other users*. Personal single-machine use is fine. Since the user intends end users to use their own subscriptions, **note this clearly in the README (Known limitations) and `docs/DECISIONS.md`** as a compliance consideration — then proceed. It is the user's call, not a reason to stop.

### Metrics & typing-trainer craft

- **WPM standard:** 1 word = **5 characters** (including spaces). `wpm = (correct_chars / 5) / minutes`; `raw = (all_typed_chars / 5) / minutes`; `accuracy = correct_keystrokes / total_keystrokes`. This is the Monkeytype/Wikipedia convention — do not invent your own word length or scores become incomparable. Optionally also show the classic **net WPM** (`gross − uncorrected_errors/minutes`) for exam-prep users, but do not double-count errors (pick the Monkeytype model as primary and keep accuracy separate).
- **Consistency:** coefficient of variation (stddev/mean) of a stream of instantaneous WPM samples, inverted and mapped to 0–100. This requires **sampling instantaneous WPM continuously** (per keystroke or ~1s windows) from the start — you cannot compute it from a single final number. Verify exact formula against the Monkeytype source/wiki.
- **Error handling is a spectrum; make it configurable.** Monkeytype exposes `stopOnError = off | letter | word` (off = type through and mark wrong chars red; letter = block on a wrong letter; word = must fix before advancing) plus difficulty tiers. Beginners benefit from forced correction; speed tests prefer type-through. Ship at least the off/letter/word choice; default to a sensible per-mode value.
- **Finger-to-key map (hard-codeable table) for finger-zone coloring & next-key logic:** left pinky `1 Q A Z` (+ Tab/Caps/Shift), ring `2 W S X`, middle `3 E D C`, index `4 5 R T F G V B`; right index `6 7 Y U H J N M`, middle `8 I K ,`, ring `9 O L .`, pinky `0 P ; / '` (+ Enter/Backspace/Shift); both thumbs = Space. Home row `ASDF`/`JKL;`, bumps on F and J. Cross-check the number/bottom rows before hardcoding colors.
- **Keyboard as training wheels, not a crutch:** the pedagogy literature is explicit that muscle memory forms only when you stop looking. The three display modes already cover this; consider (suggested) auto-fading the guide as per-key confidence rises.
- **Adaptive weak-key targeting (keybr-style, suggested enhancement for Random mode):** track per-key **accuracy and latency**; weight the random generator toward weak keys (including correct-but-slow ones — the main cause of plateaus); optionally generate pronounceable pseudo-words via a letter-transition model rather than pure noise. A spaced-repetition re-test of already-mastered keys is a nice differentiator. These are enhancements layered on the plain Random mode, not a replacement for it.
- **Results screen:** WPM-over-time line graph, per-key/finger heatmap of slow/error-prone keys, accuracy, consistency, and personal-best tracking, with a "drill your weak keys" affordance feeding back into Random mode.

### Novel-writing craft (drives the system prompt + bundled skill)

Long-form LLM fiction fails by **losing narrative purpose and drifting on continuity** (names/tense/POV silently mutate; phrasings repeat), not by writing bad sentences. Counter both with structure and named constraints:

- **Continuity bible, re-injected every chapter.** The agent should maintain and output a compact bible alongside each chapter: CAST (name, role, voice quirks, physical tags, current goal, what they know), FACTS/WORLD (immutable once stated), THREADS (open questions + who pays them off), VOICE (POV, tense, register, tone — locked), TIMELINE, PLANTED (setups awaiting payoff). Feed the bible + the previous chapter's tail into each new-chapter prompt. Persist the bible with the book on disk.
- **Craft rules (hot, in the system prompt):** write in scenes with concrete specific sensory detail; use summary only for transitions/pacing (do **not** say "always show, never tell" — that produces purple prose, itself a failure mode); show emotion through action/behavior; give each character a distinct dialogue voice; vary sentence length/rhythm; advance at least one thread or reveal one thing per chapter.
- **Anti-AI-slop bans (name them):** avoid clichés and stock phrases; avoid purple/overwrought prose and adjective piles; do not over-explain or dump subtext into text; keep tense/POV consistent; be specific not generic; do not reuse pet phrases. Ban tics like "she realized that…", "little did she know", "the weight of…", "a testament to", epiphany-summary paragraphs, and tidy moral lessons at chapter/book end. (These map to the LAMP-corpus taxonomy of AI-writing idiosyncrasies; you may cite arXiv 2409.14509 in the skill.)
- **Author, not helper (counter sycophancy):** allow discomfort, ambiguity, unresolved tension, moral complexity, unhappy or open endings when earned; do not sanitize toward upbeat/inoffensive/lesson-delivering resolutions. Respect the user's premise but do not let a thin hint lower the quality bar — elevate it.
- **Sparse / blank input:** if title/premise/continuation are blank, invent decisively — pick a specific genre, POV, tense, setting, and a protagonist with a concrete want and obstacle, and commit. Prefer the particular and surprising over the statistical average. Never stall.
- **Continue-vs-conclude sensing:** judge position in the arc. Early chapters establish and complicate (try-fail cycles, rising stakes). If the user asked for few chapters, hinted at ending, or threads are largely resolved, steer toward climax and resolution and land it. Do not pad once the story is essentially told; a shorter finished book beats an endless one. End non-final chapters on a hook, the final chapter on resolution (not a moral).
- **Rewrite-an-earlier-chapter mode:** when rewriting chapter N, first identify what later chapters depend on (facts, deaths, reveals, relationships) and keep those true. If the requested change cannot slot in without contradicting downstream events, reframe it so it still fits — as a flashback, an alternate POV of the same events, a dream/rumor/unreliable retelling — rather than creating a contradiction. Update the bible.
- **Language:** write the entire chapter **directly** in the user-named target language (free-text), thinking in that language, using its native idiom/dialogue conventions/punctuation/register — do not translate from English. Hold the same craft bar regardless of language.
- **Length:** target roughly **400–900 words per chapter** (one scene to a few beats) so a chapter is a satisfying but bounded typing session. Soft target; flex slightly for a scene's needs; keep it tight.
- **Apply the full craft on every generation.** Put the complete craft (bible schema, the named forbidden list with examples, scene-vs-summary guide, pacing/hook checklists, a per-book "voice card") into the bundled `SKILL.md` **and** into the always-on system prompt, and deterministically invoke the skill on every call, so every chapter, every rewrite, and the clarifying turn are governed by the whole thing. Redundancy between the system prompt and the skill is fine and intended. Keep the craft text tight and high-signal (do not pad it), but do not drop or defer any of it. Do a silent self-check pass before returning a chapter (contradictions, reused phrasings, tense/POV, tics, "did anything actually happen").

### Supporting crates & data (verify versions on crates.io/docs.rs)

- **Markdown parsing:** `pulldown-cmark ~0.13` (pull parser; `Event`/`Tag`/`TagEnd`; `TextMergeStream` to coalesce text). Use it to turn chapter Markdown into clean typed plain text and to feed the PDF.
- **Markdown → PDF (pure Rust, preferred for self-containment):** `typst-as-lib ~0.16` embeds the Typst engine and produces publication-quality output (title page, chapter headings, justified paragraphs, page numbers) from a small template compiled to PDF bytes in-process. `printpdf`'s HTML/markdown path is an experimental stub; `genpdf` (0.2.0, 2021) is effectively unmaintained — avoid both as the high-level renderer. `pandoc` and `weasyprint` happen to be installed and may serve as optional fallbacks, but keep the primary path pure-Rust.
- **Word list (single-word mode):** bundle a curated, **permissively-licensed** newline list via `include_str!`. **Do not** bundle Monkeytype's list (GPLv3) or google-10000-english (LDC/non-commercial). Safe sources: **EFF long wordlist** (7,776 words, CC BY 3.0 US — requires attribution) and/or SCOWL / public-domain lists (12dicts, 2of4brif). Record the license in a `NOTICE`/credits file. A frequency-ranked ordering is a plus.
- **Clipboard paste:** prefer egui's built-in `Event::Paste`; add `arboard ~3.6` only if you need clipboard access outside egui's focus/event model.
- **Config & persistence:** `serde` + `toml` (human-editable settings) and/or `serde_json` (stats), located via `directories ~6` (`ProjectDirs::from(...)` → `config_dir()` for settings, `data_dir()` for books/stats). Books live as Markdown on disk (see below).
- **CLI:** `clap ~4.6` (derive) with a hidden `--dev` bool (`#[arg(long, hide = true)]`).
- **RNG:** `rand ~0.10`. Note the 0.9+ API rename: `rand::rng()`, `random_range(a..b)`, and `rand::seq::IndexedRandom::choose(&mut rng)` (old `thread_rng`/`gen_range`/`SliceRandom` will not compile).
- **Icon rasterization (build step):** `resvg`/`usvg ~0.47` (tiny-skia) to render a source SVG to PNG sizes.

---

## Reference material

There are **no local reference repos** for this build. Your reference material is:

1. The **local `claude` CLI** (`claude --help`, and small probe runs). Prefer `--help` for flag discovery. When you must probe real output, do the **minimum** runs needed and prefer a trivial prompt, because real runs consume the user's subscription usage. Do the bulk of book-path testing against a **fake `claude` binary** you write (see Testing).
2. The **verified pointers above** and their source URLs. Re-verify anything load-bearing (egui API, `claude` flags, crate versions, metric formulas) against the live source before depending on it. Verified hints beat guesses; a guess presented as fact is worse than nothing.

Do not modify anything outside this project directory.

---

## Research phase (do this before writing app code)

The user asked for "a good amount of research," so do a real, full-depth pass, but note the heavy lifting is already summarized above — your job is to **verify and deepen**, not restart from zero:

1. Re-verify the load-bearing pointers against live sources: `docs.rs/egui` (0.35 `Event`, `Painter`, `FontDefinitions`, `NativeOptions`), `cargo add` + `Cargo.lock` for actual resolved versions, `claude --help` on this machine for exact flags and event shapes (capture one real `--output-format json` and one `stream-json --verbose` run with a trivial prompt), the Monkeytype WPM/consistency definitions (`github.com/monkeytypegame/monkeytype`), and the keybr lesson/phonetic algorithm (`github.com/aradzie/keybr.com`) if you implement adaptive drilling.
2. Confirm the plugin/skill layout by actually loading a throwaway `--plugin-dir` and checking the `init` event.
3. Fold findings into the design, then write **`docs/PLAN.md`**: a short summary of findings, the chosen design, the crate list with pinned versions, and the implementation order. Check `docs/PLAN.md` against `docs/INITIAL_DESIGN.md` so nothing required is missing, then commit it **before** building.

---

## Hard requirements (non-negotiable)

1. **Rust GUI app** that builds and runs on Linux under **both X11 and Wayland** from one codebase (test X11 here; keep the Wayland path intact). Launches with a single documented command.
2. **Three keyboard-display modes** (Guide / Feedback / Hidden) exactly as described, switchable at runtime, working for both character keys and non-character keys (arrows, function keys, etc.).
3. **On-screen keyboard** that highlights the **next key** by **physical position** in Guide mode, and flashes **just-pressed** physical keys in Feedback mode.
4. **Four content modes** — Random keys (full keyboard, not just letters), Single word (bundled list), Paste text (user-provided), Book (AI-generated) — selectable, each fully functional.
5. **Live metrics** during a session and a **results screen** after: at minimum WPM (5-char convention), raw WPM, accuracy, and elapsed/progress; results include a WPM-over-time view and per-key error/slowness feedback. Consistency and personal-best tracking are strongly encouraged.
6. **Book mode, complete:**
   - A **Books manager** (in settings or its own view) to list/manage existing books and **create a new book**.
   - New-book creation takes a **title** (may be blank) and a **chat/details box** for what the story should involve (may be blank), plus a **free-text target language** field (user types the language name themselves).
   - **One chapter generated at a time** via the Claude Agent SDK (the `claude` CLI). The user **cannot** bulk-generate; to generate the next chapter they must **finish typing** all already-generated chapters.
   - After a chapter, prompt for **how the story should continue** via a **single-line** text input (kept small).
   - The agent behaves per the novel-writing craft section: it is the author; user guidance is respected but does not derail quality; it senses continue-vs-conclude; it respects a user who wants a short 2-chapter story.
   - **One clarifying turn, at most.** The agent may return a single reply (one or more questions) which the app surfaces for the user to answer in text; after that the agent must generate. If the user leaves the continuation/details **blank**, the app **confirms** they want fully AI-invented content, and if confirmed, that generation is told the agent may **not** ask any clarifying question — it just writes.
   - **100% AI-generated is allowed:** title, premise, and continuation may all be blank; the AI invents everything, including the title.
   - **Rewrite any chapter**, including the latest possibly-not-yet-typed one. Other chapters are not rewritten; the rewritten chapter must still fit the book (reframe as flashback/alternate POV/etc. if needed). Rewriting resets that chapter's typing progress.
   - **Books persist as Markdown on disk** (one book = its chapters + metadata + continuity bible).
   - **Export**: at any time, a button to **download the original Markdown** and a button to **download a generated PDF** of whatever chapters exist so far.
   - Novel generation defaults to **Opus** (`--model opus`), with a **model picker in the Settings screen** (opus/sonnet/haiku/fable) to change it.
   - The whole point: typing the AI's chapters *is* how the book gets "written" by the user.
7. **Developer mode** behind `--dev` with the three shortcut behaviors (auto-type next key / complete page / complete chapter) as described.
8. **Resilience — predictable failures degrade, never crash the process:**
   - `claude` missing / logged out / erroring / rate-limited / max-turns → clear in-app state with actionable guidance (the in-app **Connect Claude** flow; never instructions to run terminal commands), prior chapters preserved, retry possible; the app **never hangs** waiting on the CLI (run it off-thread with a timeout/cancel), and the other three content modes keep working regardless, even with no `claude` installed at all.
   - GPU/wgpu init failure → fall back to glow or exit with a clear message, not a panic.
   - Corrupt/missing book files, empty/oversized pasted text, PDF/Markdown generation errors → handled gracefully with a user-visible message.
   - Never set `ANTHROPIC_API_KEY` in the child env; never use `--bare`; keep book runs on the subscription.
9. **Settings persist** across runs (keyboard mode, content mode, error-handling mode, caret style, sound on/off, language default, etc.) in an XDG config dir.
10. **A Makefile** at the repo root is the front door (see below), and the **README** documents install/run/test around it.

---

## Suggested design (all optional — deviate freely, record deviations in `docs/DECISIONS.md`)

### Architecture / layering

- Split a **pure-logic core** (no egui) from the **UI**:
  - `core`: metrics engine (WPM/raw/accuracy/consistency with continuous sampling), the session/text-source abstraction (a `TextSource` trait implemented by Random, Word, Paste, Book), the per-key stats / adaptive model, config load/save, the **book store** (Markdown files + metadata + bible on disk), the word-list provider, and the **agent client** that spawns `claude` and parses its output.
  - `ui`: egui views per mode, the keyboard widget, results, books manager, settings.
  - `main`: `clap` parsing (incl. hidden `--dev`), wiring, logging init.
- Make the agent client depend on a small **command-runner trait** (or a configurable binary path) so tests can inject a **fake `claude`** and never touch the network or real usage. Honor an env override like `BOOKLEY_CLAUDE_BIN` for the binary path so tests and power users can point elsewhere.
- Keep the core synchronous and testable; run `claude` on a background thread and stream events back to the UI via a channel so the GUI never blocks.

### Configuration & logging

- Config in TOML under `directories` `config_dir`; books/stats under `data_dir`. Log via `tracing`/`log` + `env_logger` (or similar) to a file and/or stderr, with **assertable log lines** for the smoke tests (e.g. "session complete wpm=… acc=…", "chapter generated book=… n=…", "claude auth: logged out"). Do not spam.

### Book on disk (suggested layout)

- One directory per book under `data_dir/books/<slug>/`: `book.toml` (title, language, created, chapter list + per-chapter typed-progress state), `bible.md` (continuity notes), `chapters/NN.md` (each chapter's Markdown). Export concatenates chapters (with a title page + `# Chapter N` headings) to a single `.md` or to the Typst→PDF pipeline. This makes rewrite/typed-state/session handling straightforward and human-inspectable.

### Dependencies

- Keep dependencies minimal and **pinned** in `Cargo.toml`. Prefer the crates named above; justify additions in `docs/DECISIONS.md`. Do not pull a crate where ~20 lines of std suffice.

### Visual design direction (load the frontend-design skill; make it genuinely nice)

The user explicitly wants this to look polished. Do not ship default-egui gray. Use this direction (deviate if you can do better, but not toward a generic template):

- **Concept — "Manuscript & Ribbon":** the practice surface is a page on a writer's desk; the app is calm, literary, and mechanical. A dark, low-eye-strain workspace (long sessions) with a **light "foolscap" paper theme** as an alternative. This deliberately avoids the current AI-design defaults (warm-cream + high-contrast serif + terracotta; near-black + acid-green; broadsheet hairlines).
- **Palette (dark theme; provide a matching light theme):**
  - `ink-950` `#14110E` — base background (warm near-black charcoal).
  - `ink-850` `#201B16` — panels/cards.
  - `paper` `#E8E1D3` — primary foreground / correctly-typed text (warm off-white).
  - `ghost` `#7C7263` — untyped / upcoming text (dim), dividers.
  - `verdigris` `#3E9C8C` — **signature interactive accent**: the caret, the next-key highlight ring, active nav, primary buttons. (Aged-brass patina; reads faintly as "go" without being kelly green.)
  - `brass` `#C9A24B` — secondary accent: wordmark, book titles, chapter headings, personal-best medals, hairline flourishes.
  - `ribbon` `#C4362F` — the typewriter-ribbon red, reserved for **errors** (mistyped chars, underlined) and destructive actions. Keep it scarce so it stays meaningful.
  - Eight muted, dark-friendly finger-zone hues for the keyboard (one per finger, standard trainer convention), tuned to sit quietly on `ink-950`.
- **Type (embed open-licensed faces; record them in `NOTICE`):**
  - **Display** (wordmark, book/chapter titles): a characterful literary face — e.g. *Fraunces* or *Young Serif* — used with restraint in `brass`.
  - **Body / reading** (book prose, results copy): a comfortable serif — e.g. *Literata* or *Source Serif 4*.
  - **The typing target text itself: a clear monospace** — e.g. *JetBrains Mono* or *IBM Plex Mono* — with unambiguous glyphs (0/O, l/1/I). Monospace makes caret motion and per-character alignment legible; it is also the deliberate "mechanical" counterpoint to the literary serif. This mono-vs-serif pairing is part of the identity.
  - **UI labels / stats HUD:** a clean grotesque (e.g. *Inter* or *IBM Plex Sans*), small caps or tabular numerals for the live stats.
- **Layout:** slim top app bar (brass wordmark + the four content-mode tabs + keyboard-mode switch + settings gear) → central **typing stage** (a focused manuscript strip: upcoming text ghosted, typed text bright `paper`, errors `ribbon`+underline, a `verdigris` caret; a quiet monospace HUD of WPM / accuracy / consistency / progress above it) → the on-screen **keyboard** docked at the bottom (toggleable), drawn as **mechanical keycaps** with finger-zone tints. Results appear as an overlay (WPM-over-time line, per-key heatmap, accuracy/consistency, PB). Books manager is a **shelf** of book spines/cards with create/continue/rewrite/export actions.
- **Signature element (spend your boldness here, keep everything else quiet):** the **letterpress keyboard** — each on-screen key does a brief **ink-stamp flash** (`brass`) on press that fades via `animate_bool_with_time`, the next key in Guide mode wears a steady `verdigris` ring, and the caret is a solid block that gives a subtle "press" on each keystroke. In Book mode, a thin **spine/progress bar** fills as the chapter is typed, reinforcing "you are binding a book." One memorable idea, executed well; no other gratuitous motion.
- Respect a **reduced-motion** setting (disable the flashes/animations), keep visible keyboard focus, and make the whole thing usable at reasonable window sizes.
- **Copy:** name things by what the user controls ("Type this chapter", "Generate next chapter", "How should the story continue?"), active voice, sentence case, no marketing filler. Empty/error states give direction ("Claude isn't signed in. Run `claude auth login` in a terminal, then retry.").

### No gold-plating

Ship the spec first. Extras (adaptive engine, pseudo-word generator, sound themes, achievements, extra themes) come **only after** the Definition of Done is fully met, and only if small. A tight, finished app beats a sprawling half-built one.

---

## Icon

Generate a topical icon yourself: a source **SVG** (a keycap that reads as an open book, or a book spine with a blinking caret — tie it to `brass`/`verdigris`/`ink`), rendered to the standard PNG sizes (16/32/48/64/128/256) with `resvg`/`usvg` (or a small build script). Wire it into a Linux **`.desktop`** file and the hicolor icon theme layout, and set the window icon at runtime. **Commit the source SVG and the generation script**, not just the rendered outputs.

---

## Testing rules

- **Do not** run the full suite after every small change — too slow. Loop: implement a coherent batch → commit → run the full suite → fix breakage in follow-up commits. Cheap targeted checks while developing (one test file, `cargo check`, `clippy`) are encouraged.
- **Unit tests for the core** with external dependencies mocked, covering success and failure paths:
  - Metrics: known keystroke sequences → known WPM/raw/accuracy/consistency (including the 5-char convention and an all-errors case).
  - Text sources: Random generator honors the requested key pool (and excludes dev keys); Word source draws from the list; Paste source reproduces input exactly.
  - Adaptive model (if built): weak keys get up-weighted.
  - Book store: create → add chapter → typed-progress → rewrite → reload round-trips on disk; export produces valid Markdown.
  - **Agent client: feed recorded `stream-json` / `json` lines (from a fixture or the fake `claude`) and assert it extracts the chapter text, `session_id`, and classifies `error_max_turns` / `rate_limit` / logged-out correctly.** This must never call the real `claude`.
  - PDF: generating from a small book yields bytes starting with `%PDF` and non-trivial length.
- **At least one real end-to-end smoke test of the running app, verified from a script.** X11 is available here (`DISPLAY=:0`). Options (pick what works): a `--smoke`/`--selftest` subcommand that boots the app, runs a scripted typing session through the real pipeline using the dev auto-type path, asserts the resulting stats via logs/exit code, and exits non-zero on failure; plus, if feasible, an actual windowed launch that renders a frame and captures a screenshot for inspection. For the book path, the smoke test points `BOOKLEY_CLAUDE_BIN` at a **fake `claude`** script that emits canned `stream-json`, so it exercises the full spawn/parse/persist/typing/export flow **without consuming subscription usage or requiring auth**.
- Anything genuinely unverifiable here (Wayland rendering on this X11-only box; a *live* real-`claude` generation, which needs auth and burns usage) goes in the README under **Known limitations** with a note on how to run it manually — never a question. Provide an opt-in `make live-book-smoke` (or similar) that a human can run against the real CLI.
- Before finishing: **full suite green**, and the README's install-and-run steps work when followed literally.

---

## Makefile (repo-root front door)

Provide a `Makefile` whose targets actually work on a fresh clone. Mandatory: a **`help`** target listing every target with a one-line description, ideally the default so bare `make` prints help. Suggested targets: `deps`/`setup` (verify toolchain, note no `sudo` needed for the Rust build), `install-claude` (install the Claude Code CLI on a fresh Debian/Ubuntu box, see below), `build`, `run`, `dev` (runs with `--dev`), `test`, `smoke` (scriptable e2e with the fake claude), `live-book-smoke` (opt-in, real claude), `lint` (`cargo clippy`/`fmt --check`), `icons` (regenerate icon PNGs), `clean`. The README points at these targets as the primary way to install, run, and test rather than duplicating raw commands.

**`install-claude` target (the user asked for this — a target box may not have Claude Code installed).** Book mode needs the `claude` CLI. Provide a target that installs it from Anthropic's **official APT repo** on Debian/Ubuntu (verified 2026-07-10 against `https://code.claude.com/docs/en/setup`; re-verify at build time). The commands (run with sudo):

```
sudo install -d -m 0755 /etc/apt/keyrings
sudo curl -fsSL https://downloads.claude.ai/keys/claude-code.asc -o /etc/apt/keyrings/claude-code.asc
echo "deb [signed-by=/etc/apt/keyrings/claude-code.asc] https://downloads.claude.ai/claude-code/apt/stable stable main" | sudo tee /etc/apt/sources.list.d/claude-code.list
sudo apt-get update
sudo apt-get install -y claude-code
```

Notes: the package is `claude-code`; the repo auto-serves amd64/arm64 (no explicit `arch=` needed); the signing-key fingerprint is `31DD DE24 DDFA B679 F42D 7BD2 BAA9 29FF 1A7E CACE` (optionally verify with `gpg --show-keys /etc/apt/keyrings/claude-code.asc`). If apt is unavailable, fall back to the native installer `curl -fsSL https://claude.ai/install.sh | bash` (or npm `@anthropic-ai/claude-code`). After install, authentication happens through the app's in-app **Connect Claude** flow (see the in-app authentication section); terminal login (`claude auth login` / `claude setup-token`) remains a manual fallback for power users only; check state with `claude auth status`. Make the target **idempotent and opt-in**: if `claude` is already present, no-op; never run it automatically during the build, and never require it for the non-book modes. Document in the README that it is Debian/Ubuntu-only and touches system state via sudo.

---

## Git rules

- Project root is the current working directory. If it is not already a git repo, `git init`.
- Use the git identity already configured on this box (global `user.name`/`user.email`, currently `Zoffix Znet <git@zoffix.com>`). **Do not** override or touch it. Every commit carries that identity.
- Commit in reasonably sized, **feature-shaped** chunks as work progresses, with short imperative subject lines and a brief body when a decision needs explaining.
- After committing, **push automatically only if a remote (`origin`) is configured**. There is currently **no remote** — so **skip the push, do not create one**, and note the skip in `docs/DECISIONS.md`. (The user will add a remote later.)
- **No AI attribution anywhere:** no `Co-Authored-By`, no "generated with" trailers, no tool names in commit messages, code comments, or docs.

## Writing style

- **No em-dashes in commit messages** (use commas, parentheses, colons, or separate sentences). This applies to commit messages only — do not extend it to other files.
- Docs and commits read like a busy human engineer wrote them: plain, direct, specific. No emoji, no marketing adjectives, no filler.

---

## Definition of done

Re-read `docs/INITIAL_DESIGN.md` top to bottom before running this checklist, to catch anything the checklist glosses over.

- [ ] `docs/PLAN.md` and `docs/DECISIONS.md` exist and reflect what actually happened. `README.md` is the only doc at the repo root; everything else is under `docs/`.
- [ ] The app **launches with a single command** documented in the README, and runs on X11 here (Wayland path intact and documented).
- [ ] A `Makefile` exists at the repo root, `make help` works, and every target does what it claims on a fresh clone.
- [ ] **Every hard requirement is met:** three keyboard-display modes; four content modes; on-screen keyboard with physical next-key highlight and just-pressed flash; live metrics + results screen with WPM-over-time and per-key feedback; full Book mode (create with optional-blank title/premise/language, one-chapter-at-a-time gated on finished typing, single-line continue prompt, one-clarifying-turn rule + blank-input confirmation that disables clarifying, 100%-AI allowed, rewrite-any-chapter-that-still-fits, Markdown-on-disk, Markdown + PDF export); developer `--dev` shortcuts; resilience/degradation for all listed failure modes; subscription-only `claude` usage (no `ANTHROPIC_API_KEY`, no `--bare`); **in-app Claude authentication** (Connect Claude flow with a clickable link; the end user never has to run a terminal command); persisted settings.
- [ ] Expert novel-writing craft is preloaded into the agent via a **bundled skill/plugin dir** (`--plugin-dir`, deterministically invoked) **and** the full craft in the always-on system prompt, applied unconditionally to every chapter, rewrite, and clarifying turn, per the craft section.
- [ ] A topical icon (source SVG + generated PNGs + `.desktop` wiring) exists, with the source/script committed.
- [ ] Tests pass; the **final full-suite run is green**. Core unit tests + the scriptable e2e smoke (with a fake `claude`) both run via `make`.
- [ ] The README covers install, configuration, running, and **Known limitations** (Wayland untested here; live book generation needs a connected Claude subscription (via the in-app Connect Claude flow) and consumes subscription usage; the Anthropic ToS note on distributing subscription-driven apps), all built around the make targets.
- [ ] Git history is clean, feature-chunked, authored by the box's configured identity, with **no AI attributions** and **no em-dashes in any commit message**. No remote configured, so no push (noted in `docs/DECISIONS.md`).

After the checklist: re-read `docs/INITIAL_DESIGN.md` one final time to catch anything missed, run the full test suite one last time, fix what it turns up, commit, and end with a short plain summary of what was built and any decisions worth knowing about.

**At no point ask whether to keep going. The answer is always yes.** This document, `docs/INITIAL_DESIGN.md`, is the job; when in doubt, re-read it and continue until the Definition of Done is fully met.
