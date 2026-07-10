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

Requirements: a Rust toolchain (https://rustup.rs), the usual Linux GUI libraries
(X11/Wayland, OpenGL or Vulkan), and the ALSA dev headers for the typewriter key sound
(`sudo apt install libasound2-dev` on Debian/Ubuntu). `make deps` sanity checks the
toolchain and libraries.

To install for your user (so the clone can be deleted afterwards):

```
make install
```

That puts a single self-contained binary at `~/.local/bin/bookley` (fonts, sounds, the
word list, and the novelist plugin are all embedded) plus a desktop entry and icons.
Make sure `~/.local/bin` is on your PATH. Installing a newer version over an old one
replaces those same files; `make uninstall` removes exactly them. Your books, settings,
and stats live under `~/.local/share/bookleykeytrainer` and `~/.config`, and are never
touched by install or uninstall.

## What's inside

Two independent axes define a practice session, both switchable at runtime from the top
bar (and in Settings):

- **Keyboard display**: *Guide* (the on-screen full-size keyboard highlights the next
  key to press, plus Shift when the character needs it), *Feedback* (no hint;
  just-pressed keys flash briefly), *Hidden* (no keyboard). The numpad can be hidden in
  Settings.
- **Content**: *Random keys* (a timed drill over the whole physical keyboard, including
  arrows, function keys, and the nav cluster), *Single word* (a timed Monkeytype-style
  drill over a flowing stream of words from the bundled list), *Paste text* (type
  exactly what you paste), *Book* (AI-generated fiction, see below).

Every mode starts behind a "Press Space to start" gate: the clock only runs once you
press Space, and that press is never counted as typing. Word and Random drills run for a
configurable duration (30s to 5m presets, pickable right on the start gate and in
Settings as the default; default 2 minutes) and show results when time is up. In Random
mode Backspace is a drillable key like any other. Any session can be paused (Space
resumes); paused time never counts toward the metrics, and the target text is veiled so
it cannot be read ahead. Paste and Book sessions have a "Reset stats" control that
zeroes the timer and metrics without losing your place in the text.

Every keypress lands with a real typewriter sound: three distinct CC0 (public-domain)
single-key recordings for ordinary keys and two deeper Hermes Precisa 305 space-bar and
typebar thunks for Space, Enter, and Backspace, bundled under `assets/sounds/`, embedded
in the binary, and played with slight random pitch/volume variation (see NOTICE for
sources). The top-bar Sound switch controls the running session; Settings sets whether
it starts on or off (on by default). If a bundled recording ever fails to decode the old
synthesized click takes over automatically, and on a machine with no audio device the
app simply stays silent.

Live WPM (5-characters-per-word convention), raw WPM, accuracy, and consistency are
shown while you type; the results screen adds a WPM-over-time graph (labeled time axis
and a solid-vs-dashed legend), your slowest and most error-prone keys (errors made while
each key was expected, plus its average inter-keystroke time), net WPM, and personal
bests, and Enter starts the next drill. Random mode quietly weights future rounds toward
the keys you miss. Error handling is configurable: type-through, stop on letter, or stop
on word (fix the word before you can leave it). Mistyped text is marked with a bright
error ink plus an underline and a background tint, so the state never depends on color
alone.

Typing targets are normalized to plain keystrokes: accented letters fold to ASCII
(E for É), smart dashes/quotes/ellipses become their plain equivalents, exotic Unicode
spaces become regular spaces, and anything unmappable is dropped. This applies to pasted
text and book chapters; the book files on disk and the exports keep the original
Unicode. The default theme is the light "foolscap" one; a saved theme choice wins.

Settings, stats, and books persist in the XDG directories
(`~/.config/bookleykeytrainer`, `~/.local/share/bookleykeytrainer`; exact paths vary by
platform conventions).

## Book mode

Create a book with a title, a description of what the story should involve, and a target
language (free text). All three may be left blank; if you leave the direction blank the
app asks you to confirm, and the author invents everything, including the title.

- One chapter is generated at a time, sized like a chapter in a printed novel (as long
  as the scene work demands; no fixed word target). To unlock the next chapter you must
  finish typing every chapter so far. You do not have to type a chapter in one sitting:
  progress is saved continuously, and returning to a chapter resumes one paragraph
  before where you left off as a refresher.
- The app remembers the book you were working on: launching in Book mode reopens it and
  drops you straight onto the typing stage of its next chapter, Space gate armed, no
  clicking through the Books page. (With nothing left to type it opens the book's page
  instead.)
- After each chapter a single-line prompt asks how the story should continue. The author
  may ask one round of clarifying questions at most; answer in the app and it writes.
- Ticking "Make this chapter the last chapter of the book" tells the author to land the
  ending in that chapter; the book is then marked finished (rewrites stay possible).
- Any chapter can be rewritten (with an instruction), including the latest untyped one;
  the rewrite must still fit the rest of the book, and rewriting resets that chapter's
  typing progress.
- Books live on disk as human-readable Markdown plus a continuity bible; export the
  whole book as Markdown or a typeset PDF at any time. Exports contain the book only
  (cover, title page, chapters); the premise and language you entered are generation
  inputs and never appear in them. Exports open in your system's default viewer and
  land in the app's data dir under `exports/` (the app shows the exact path).
- "Generate cover" on the book page asks Claude to design a cover as a self-contained
  SVG from the book's actual title and text, which the app validates and renders to a
  PNG; it appears on the book page and as the full-bleed first page of the PDF. If a
  design cannot be rendered the app falls back to a clean typographic cover, so the
  button always yields one. Regenerating replaces it.
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
  Markdown and PDF), cover generation (canned SVG rendered, invalid SVG hitting the
  typographic fallback), failure classification (rate limit, logged out, max turns),
  the watchdog that kills a hung CLI, and the whole Connect Claude flow against a fake
  PTY. It prints assertable `smoke: ... ok` lines and exits non-zero on failure.
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
- **PDF export of non-Latin scripts depends on system fonts.** The embedded fonts cover
  Latin/Cyrillic-range text; for CJK, Arabic, Devanagari and similar scripts the
  exporter searches the system's fonts, so real glyphs appear only if a matching font
  is installed (otherwise boxes may show). Hyphenation follows the book's language
  field, falling back to English.
- The CLI's output is an unstable interface. The Connect Claude flow scrapes it
  defensively and fails with a clear in-app message (never a hang) if the format
  changes; the fallback for power users is signing in with the CLI directly, which the
  app auto-detects.
