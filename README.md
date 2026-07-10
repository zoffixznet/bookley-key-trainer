# Bookley Key Trainer

A desktop typing trainer for Linux (X11 and Wayland) with a twist: its Book mode
generates a real novel, one chapter at a time, with Claude, and the only way to get the
next chapter is to finish typing the ones you have. When you are done practicing, you
have a book you can export as Markdown or PDF.

Written in Rust with egui/eframe. Everything is driven from the `Makefile`.

## Quick start

```
make run
```

That builds the release binary and launches the app. `make help` lists every target.

Requirements: a Rust toolchain (https://rustup.rs) and the usual Linux GUI libraries
(X11/Wayland, OpenGL or Vulkan). No sudo is needed for the build. `make deps` sanity
checks the toolchain.

## What's inside

Two independent axes define a practice session, both switchable at runtime from the top
bar (and in Settings):

- **Keyboard display**: *Guide* (on-screen keyboard highlights the next key to press),
  *Feedback* (no hint; just-pressed keys flash briefly), *Hidden* (no keyboard).
- **Content**: *Random keys* (the whole physical keyboard, including arrows, function
  keys, and the nav cluster), *Single word* (bundled word list), *Paste text* (type
  exactly what you paste), *Book* (AI-generated fiction, see below).

Live WPM (5-characters-per-word convention), raw WPM, accuracy, and consistency are shown
while you type; a results screen afterward adds a WPM-over-time graph, your slowest and
most error-prone keys, net WPM, and personal bests. Random mode quietly weights future
rounds toward the keys you miss. Error handling is configurable: type-through, stop on
letter, or stop on word (fix the word before you can leave it).

Settings, stats, and books persist in the XDG directories
(`~/.config/bookleykeytrainer`, `~/.local/share/bookleykeytrainer`; exact paths vary by
platform conventions).

## Book mode

Create a book with a title, a description of what the story should involve, and a target
language (free text). All three may be left blank; if you leave the direction blank the
app asks you to confirm, and the author invents everything, including the title.

- One chapter is generated at a time (roughly 400-900 words). To unlock the next
  chapter you must finish typing every chapter so far.
- After each chapter a single-line prompt asks how the story should continue. The author
  may ask one round of clarifying questions at most; answer in the app and it writes.
- Any chapter can be rewritten (with an instruction), including the latest untyped one;
  the rewrite must still fit the rest of the book, and rewriting resets that chapter's
  typing progress.
- Books live on disk as human-readable Markdown plus a continuity bible; export the
  whole book as Markdown or a typeset PDF at any time (files land in the app's data dir
  under `exports/`, and the app shows the exact path).
- Generation defaults to the Opus model; Settings has a model picker (opus, sonnet,
  haiku, fable).

### Connecting Claude

Book mode uses the Claude Code CLI with your Claude subscription. The app never asks you
to touch a terminal to sign in:

1. Install the CLI once if it is missing: `make install-claude` (Debian/Ubuntu, uses
   Anthropic's official APT repository, needs sudo; it is a no-op when `claude` is
   already installed). Everything except Book mode works without the CLI entirely.
2. In the app, open Book mode and click **Connect Claude**. The app runs Anthropic's
   sign-in flow for you: your browser opens (or you click the link the app shows), you
   press Authorize on claude.ai, paste the short code back into the app, and you are
   connected. The captured token is stored with owner-only permissions in the app's
   config dir and used for all generation.

If the CLI is already signed in (say, on a developer machine), the app detects that and
skips the connect flow. Generation runs never set `ANTHROPIC_API_KEY` and never use API
billing; usage draws from your subscription.

## Developer mode

`make dev` (or `bookley --dev`) enables developer shortcuts and shows a DEV badge:

- **F9**: auto-type the next expected key (hold it to type through a whole chapter).
- **F10**: complete the current page of text.
- **F12**: complete the whole chapter/text.

Random mode excludes F9/F10/F12 from its key pool so the shortcuts never collide.

## Testing

- `make test` runs the unit suite plus integration tests. The book paths are tested
  against `tests/fake_claude.sh`; nothing touches the network or your subscription.
- `make smoke` runs a headless end-to-end self-test through the real pipeline: typed
  sessions in every mode, a full book cycle (generate, parse, persist, type, export
  Markdown and PDF), failure classification (rate limit, logged out, max turns), the
  watchdog that kills a hung CLI, and the whole Connect Claude flow against a fake PTY.
  It prints assertable `smoke: ... ok` lines and exits non-zero on failure.
- `make screenshot` boots the real windowed app, renders a few frames, saves
  `smoke-screenshot.png`, and exits; useful to verify rendering on a new box.
- `make lint` runs clippy (warnings are errors) and a rustfmt check.
- `make live-book-smoke` is opt-in and generates one real chapter with the real
  `claude` CLI to verify the live path end to end. It needs a connected Claude and
  consumes subscription usage; nothing else in the build or tests does.

## Icon and desktop entry

The icon source is `assets/icon/bookley.svg`; `make icons` regenerates the PNGs (hicolor
layout plus the embedded window icon) via the `gen-icons` helper binary.
`make desktop-install` installs the `.desktop` entry and icons for the current user.

## Known limitations

- **Wayland is untested here.** This machine's session is X11, where the app is verified
  (including scripted screenshots). The same binary carries eframe/winit's Wayland
  backend unchanged; run `make run` on a Wayland session to verify. If the compositor
  misbehaves with wgpu, the app automatically retries with the OpenGL (glow) renderer;
  `WINIT_UNIX_BACKEND=x11` forces XWayland as a last resort.
- **Live book generation is not exercised by the test suite.** It requires a connected
  Claude subscription and consumes real usage, so it is behind the opt-in
  `make live-book-smoke` (and was verified manually during development). Everything else
  runs against the bundled fake CLI.
- **Anthropic terms note.** Anthropic's SDK documentation states that, without prior
  approval, third-party developers may not offer claude.ai sign-in / subscription-based
  usage in products *distributed to other users*. Using Bookley yourself with your own
  subscription is fine; if you plan to distribute it, that clause is your call to
  resolve with Anthropic first. (Also verified at build time: `claude -p` / Agent SDK
  usage currently draws from the subscription's usage limits; Anthropic paused the
  planned move to separate credits.)
- **`make install-claude` is Debian/Ubuntu-oriented** (APT repository, sudo). On other
  distributions it falls back to Anthropic's native installer script.
- The CLI's output is an unstable interface. The Connect Claude flow scrapes it
  defensively and fails with a clear in-app message (never a hang) if the format
  changes; the fallback for power users is signing in with the CLI directly, which the
  app auto-detects.
