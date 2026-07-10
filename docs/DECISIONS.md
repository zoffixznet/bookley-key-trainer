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
- Full craft on every call (per updated spec, line 79/80/110/280): the COMPLETE novelist
  craft lives in the always-on system prompt (`--append-system-prompt`) AND in the bundled
  `SKILL.md`, and we invoke the skill deterministically with `/novelist:write-chapter ...`
  at the top of every prompt (chapter, rewrite, and the clarifying turn). We do not rely on
  description-based auto-surfacing, and we do not trim/defer/summarize craft to save tokens.
  Redundancy between prompt and skill is intended. Tokens are explicitly not a concern.

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

## Fonts / assets

- Fonts: to keep the repo self-contained and avoid shipping large binary font files we may
  not have licenses staged for, the app embeds fonts only if present under
  `assets/fonts/`; otherwise it falls back to egui's bundled fonts and applies the palette
  + sizing so the identity still reads. Any embedded font's license is recorded in NOTICE.
  (See NOTICE / README for the current state.)

## Content / metrics

- WPM uses the Monkeytype/Wikipedia 5-char convention. Consistency is the coefficient of
  variation of instantaneous WPM samples (one sample per correct keystroke), inverted and
  mapped to 0-100, per the Monkeytype definition.
- Random-keys mode draws from the full physical keyboard including non-character keys
  (arrows, function keys, nav cluster, Tab/Enter/Esc). Dev keys (F9/F10/F12) are excluded
  from the Random pool so they never clash with dev shortcuts.
