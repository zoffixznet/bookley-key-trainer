//! Prompt construction for book generation.
//!
//! Per the spec (INITIAL_DESIGN.md lines 79-80, 110): the COMPLETE novelist craft is
//! present in the always-on system prompt AND in the bundled SKILL.md, and we invoke the
//! skill deterministically (`/novelist:write-chapter ...`) on EVERY call: chapter,
//! rewrite, and the one clarifying turn. Tokens are explicitly not a concern; we never
//! trim or defer the craft.
//!
//! The agent is told to wrap output in explicit markers so parsing is deterministic
//! regardless of prose content:
//!   ===TITLE===   (one line, the chapter title)
//!   ===CHAPTER=== (the chapter prose, in the target language)
//!   ===BIBLE===   (the updated continuity bible)
//!   ===END===
//! For the clarifying turn it instead returns:
//!   ===QUESTIONS===
//!   (one or more questions)
//!   ===END===

use super::store::{Book, BookMeta};

/// The full novelist craft. This text is shipped verbatim both here (into the always-on
/// system prompt) and inside the bundled SKILL.md. Keep the two in sync.
pub const NOVELIST_CRAFT: &str = include_str!("craft.md");

/// The always-on system prompt: identity + the complete craft + the output contract.
pub fn system_prompt() -> String {
    format!(
        "You are Bookley's resident novelist: a careful, ambitious literary author writing \
one chapter at a time for a reader who will type the chapter out by hand. You are the \
author, not an assistant taking dictation. Respect the user's premise and guidance, but \
never let a thin hint lower the quality bar; elevate it.\n\n\
The complete craft below governs EVERY chapter, EVERY rewrite, and the clarifying turn. \
It is not optional and not situational.\n\n\
{craft}\n\n\
## Output contract (MANDATORY, exact)\n\
Return ONLY the marked blocks, nothing before or after. Do not add commentary.\n\
When writing or rewriting a chapter, return exactly:\n\
===TITLE===\n\
<a short chapter title in the target language, or a single hyphen if none>\n\
===CHAPTER===\n\
<the chapter prose, written directly in the target language>\n\
===BIBLE===\n\
<the full updated continuity bible: CAST, FACTS/WORLD, THREADS, VOICE, TIMELINE, PLANTED>\n\
===END===\n\n\
If (and only if) you were explicitly allowed to ask and you genuinely need one round of \
clarification before you can write well, return instead exactly:\n\
===QUESTIONS===\n\
<one or more concise questions on separate lines>\n\
===END===\n\
Never ask more than once. If you were told not to ask, do not ask; invent decisively and write.",
        craft = NOVELIST_CRAFT
    )
}

/// The directive injected when the reader marks this as the last chapter.
pub const CONCLUDE_DIRECTIVE: &str = "IMPORTANT: the reader has decided this is the LAST \
chapter of the book. Conclude the story IN THIS CHAPTER: steer to the climax, resolve or \
deliberately land the open threads, and end on resolution, not a cliffhanger and not a \
moral. Do not set up anything new that this chapter cannot pay off.";

/// Build the per-turn user prompt for generating chapter `n` of `book`.
///
/// `continuation` is the user's single-line "how should the story continue" (may be empty).
/// `allow_clarify` gates the one clarifying turn; when false the agent must write directly.
/// `story_so_far_path` optionally points at a file with the full prior text (referenced,
/// not pasted, to respect the stdin cap for long novels).
/// `conclude` marks this as the book's final chapter: the agent must land the ending.
pub fn chapter_prompt(
    book: &Book,
    n: usize,
    continuation: &str,
    allow_clarify: bool,
    story_so_far_path: Option<&str>,
    conclude: bool,
) -> String {
    let meta = &book.meta;
    let bible = book.read_bible();
    let tail = book.previous_chapter_tail(n);
    let mut p = String::new();

    // Deterministic, explicit skill invocation on every call.
    p.push_str("/novelist:write-chapter\n\n");
    p.push_str(&format!(
        "Task: write chapter {n} of this book. Apply the full novelist craft (it is also \
in your system prompt and in the skill you just invoked).\n\n"
    ));

    p.push_str(&book_header(meta));

    if n == 1 {
        p.push_str(
            "This is the first chapter. Establish voice, POV, tense, setting, and a \
protagonist with a concrete want and a concrete obstacle. Open in a scene.\n\n",
        );
        if meta.title.trim().is_empty() {
            p.push_str(
                "The book has no title yet: choose one and put it on the FIRST line of \
the ===BIBLE=== block as `BOOK-TITLE: <your title>`.\n\n",
            );
        }
    } else {
        p.push_str(&format!(
            "This continues an existing book. You are writing chapter {n}. Keep continuity \
with everything already established.\n\n"
        ));
    }

    if !bible.trim().is_empty() {
        p.push_str("## Continuity bible (authoritative; keep it true, then update it)\n");
        p.push_str(bible.trim());
        p.push_str("\n\n");
    }

    if !tail.trim().is_empty() {
        p.push_str("## Tail of the previous chapter (for seamless continuation)\n");
        p.push_str(tail.trim());
        p.push_str("\n\n");
    }

    if let Some(path) = story_so_far_path {
        p.push_str(&format!(
            "## Full story so far\nThe complete text of every prior chapter is in the file \
`{path}`. Read it if you need more than the tail above; do not contradict it.\n\n"
        ));
    }

    if continuation.trim().is_empty() {
        if allow_clarify {
            p.push_str(
                "The reader gave no direction for this chapter. You MAY ask one round \
of clarifying questions if it will meaningfully improve the chapter; otherwise invent \
decisively and write.\n\n",
            );
        } else {
            p.push_str(
                "The reader gave no direction and has confirmed they want you to \
invent everything. You may NOT ask any clarifying question. Decide the direction yourself \
and write the chapter now.\n\n",
            );
        }
    } else {
        p.push_str("## How the reader wants the story to continue\n");
        p.push_str(continuation.trim());
        p.push_str("\n\n");
        if allow_clarify {
            p.push_str(
                "You MAY ask one round of clarifying questions only if genuinely \
necessary; otherwise write the chapter now.\n\n",
            );
        } else {
            p.push_str("Do not ask any clarifying question; write the chapter now.\n\n");
        }
    }

    if conclude {
        p.push_str(CONCLUDE_DIRECTIVE);
        p.push_str("\n\n");
    } else {
        p.push_str(&conclude_guidance(meta, n));
    }
    p.push_str(&length_and_language(meta));
    p.push_str("\nReturn only the marked blocks per the output contract.");
    p
}

/// Build the prompt to rewrite an existing chapter `n`, keeping downstream continuity true.
pub fn rewrite_prompt(book: &Book, n: usize, instruction: &str) -> String {
    let meta = &book.meta;
    let bible = book.read_bible();
    let existing = book.read_chapter(n).unwrap_or_default();
    let mut p = String::new();

    p.push_str("/novelist:write-chapter\n\n");
    p.push_str(&format!(
        "Task: REWRITE chapter {n} of this book. Apply the full novelist craft.\n\n"
    ));
    p.push_str(&book_header(meta));

    // Downstream dependencies: later chapters that must stay consistent.
    let later: Vec<usize> = meta
        .chapters
        .iter()
        .filter(|c| c.n > n)
        .map(|c| c.n)
        .collect();
    if !later.is_empty() {
        p.push_str(&format!(
            "IMPORTANT: chapters {later:?} come AFTER this one and already exist. Before \
rewriting, identify what they depend on (facts, deaths, reveals, relationships) and keep \
those true. If the requested change cannot slot in without contradicting downstream \
events, reframe it so it still fits (a flashback, an alternate POV of the same events, a \
dream, a rumor, an unreliable retelling) rather than creating a contradiction.\n\n"
        ));
    }

    if !bible.trim().is_empty() {
        p.push_str("## Continuity bible (authoritative)\n");
        p.push_str(bible.trim());
        p.push_str("\n\n");
    }

    p.push_str("## The existing chapter to rewrite\n");
    p.push_str(existing.trim());
    p.push_str("\n\n");

    p.push_str("## What the reader wants changed\n");
    if instruction.trim().is_empty() {
        p.push_str(
            "(No specific instruction: improve this chapter on its own terms while \
keeping every downstream fact true.)\n\n",
        );
    } else {
        p.push_str(instruction.trim());
        p.push_str("\n\n");
    }

    p.push_str(
        "Do not ask clarifying questions; rewrite the chapter now. Update the bible \
to reflect the rewrite. ",
    );
    p.push_str(&length_and_language(meta));
    p.push_str("\nReturn only the marked blocks per the output contract.");
    p
}

fn book_header(meta: &BookMeta) -> String {
    let title = if meta.title.trim().is_empty() {
        "(untitled: you may choose the book's title)".to_string()
    } else {
        meta.title.trim().to_string()
    };
    let mut s = format!("## Book\nTitle: {title}\n");
    let lang = if meta.language.trim().is_empty() {
        "(unspecified: default to English unless the premise implies otherwise)"
    } else {
        meta.language.trim()
    };
    s.push_str(&format!("Target language: {lang}\n"));
    if !meta.premise.trim().is_empty() {
        s.push_str(&format!("Premise / details: {}\n", meta.premise.trim()));
    } else {
        s.push_str("Premise / details: (none given; invent a specific, particular premise)\n");
    }
    s.push('\n');
    s
}

fn conclude_guidance(meta: &BookMeta, n: usize) -> String {
    // Sense continue-vs-conclude. If the premise hints at a short story, or several
    // chapters exist and threads are winding down, steer toward landing it.
    let hints_short = {
        let low = meta.premise.to_lowercase();
        low.contains("short")
            || low.contains("two chapter")
            || low.contains("2 chapter")
            || low.contains("2-chapter")
            || low.contains("brief")
    };
    if hints_short && n >= 2 {
        "Judge the arc: the reader signalled a short book. Steer toward climax and \
resolution and land it in this chapter or the next. End the final chapter on resolution, \
not a moral. Do not pad.\n\n"
            .to_string()
    } else if n == 1 {
        "This is early: establish and complicate. End on a hook that pulls into the next \
chapter.\n\n"
            .to_string()
    } else {
        "Judge your position in the arc. If threads are largely resolved or the story is \
essentially told, steer toward climax and resolution and land it; a shorter finished book \
beats an endless one. Otherwise complicate and raise stakes, and end on a hook. Do not \
pad.\n\n"
            .to_string()
    }
}

fn length_and_language(meta: &BookMeta) -> String {
    let lang = if meta.language.trim().is_empty() {
        "English".to_string()
    } else {
        meta.language.trim().to_string()
    };
    format!(
        "Length: this is a full-size novel; make the chapter as substantial as a chapter \
in a printed novel, as long as the scene work demands and no longer. Do not pad, and do \
not compress or stop early; there is no fixed word target, but a three-page chapter is \
not a novel chapter. Write the entire chapter directly in {lang}, thinking in that \
language and using its native idiom, dialogue conventions, punctuation, and register; do \
not translate from English. "
    )
}

/// Extract the two marker-delimited forms from a raw agent reply.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedReply {
    /// A generated/rewritten chapter.
    Chapter {
        title: String,
        prose: String,
        bible: String,
    },
    /// The single clarifying turn.
    Questions(String),
    /// No markers found; treat the whole thing as chapter prose (best-effort fallback).
    Fallback(String),
}

/// Parse the agent's reply text into a `ParsedReply`.
pub fn parse_reply(raw: &str) -> ParsedReply {
    let text = raw.trim();
    if text.contains("===QUESTIONS===") {
        let q = between(text, "===QUESTIONS===", "===END===")
            .unwrap_or(text)
            .trim()
            .to_string();
        return ParsedReply::Questions(q);
    }
    if text.contains("===CHAPTER===") {
        let title = between(text, "===TITLE===", "===CHAPTER===")
            .map(|s| s.trim().to_string())
            .filter(|s| s != "-" && !s.is_empty())
            .unwrap_or_default();
        let prose = between(text, "===CHAPTER===", "===BIBLE===")
            .or_else(|| between(text, "===CHAPTER===", "===END==="))
            .unwrap_or(text)
            .trim()
            .to_string();
        let bible = between(text, "===BIBLE===", "===END===")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        return ParsedReply::Chapter {
            title,
            prose,
            bible,
        };
    }
    ParsedReply::Fallback(text.to_string())
}

/// Extract an agent-invented book title from a bible ("BOOK-TITLE: ..." line), if any.
pub fn book_title_from_bible(bible: &str) -> Option<String> {
    for line in bible.lines() {
        let line = line.trim().trim_start_matches(['-', '*', ' ']);
        if let Some(rest) = line.strip_prefix("BOOK-TITLE:") {
            let t = rest.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Return the text strictly between the first `start` and the following `end` marker.
fn between<'a>(hay: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let s = hay.find(start)? + start.len();
    let rest = &hay[s..];
    let e = rest.find(end).unwrap_or(rest.len());
    Some(&rest[..e])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_contains_full_craft_and_contract() {
        let sp = system_prompt();
        assert!(sp.contains("===CHAPTER==="));
        assert!(sp.contains("===QUESTIONS==="));
        // A distinctive craft phrase must be present (redundant with the skill by design).
        assert!(sp.to_lowercase().contains("continuity bible"));
        assert!(sp.contains("author, not an assistant"));
    }

    #[test]
    fn chapter_prompt_invokes_skill_deterministically() {
        let store = tmp_store();
        let book = store.create("T", "English", "a heist", false).unwrap();
        let p = chapter_prompt(&book, 1, "make it tense", true, None, false);
        assert!(p.starts_with("/novelist:write-chapter"));
        assert!(p.contains("chapter 1"));
        assert!(p.contains("make it tense"));
        assert!(p.contains("MAY ask one round"));
    }

    #[test]
    fn blank_confirmed_disables_clarifying() {
        let store = tmp_store();
        let book = store.create("", "", "", true).unwrap();
        let p = chapter_prompt(&book, 1, "", false, None, false);
        assert!(p.contains("may NOT ask") || p.contains("not ask any clarifying"));
        assert!(!p.contains("MAY ask one round"));
    }

    #[test]
    fn parse_chapter_reply() {
        let raw = "junk before\n===TITLE===\nThe Arrival\n===CHAPTER===\nIt began at dusk.\n\
More prose.\n===BIBLE===\nCAST: Mara\n===END===\njunk after";
        match parse_reply(raw) {
            ParsedReply::Chapter {
                title,
                prose,
                bible,
            } => {
                assert_eq!(title, "The Arrival");
                assert!(prose.contains("It began at dusk."));
                assert!(prose.contains("More prose."));
                assert!(!prose.contains("junk"));
                assert_eq!(bible, "CAST: Mara");
            }
            other => panic!("expected chapter, got {other:?}"),
        }
    }

    #[test]
    fn parse_questions_reply() {
        let raw = "===QUESTIONS===\nWhat POV?\nHow many chapters?\n===END===";
        match parse_reply(raw) {
            ParsedReply::Questions(q) => {
                assert!(q.contains("What POV?"));
                assert!(q.contains("How many chapters?"));
            }
            other => panic!("expected questions, got {other:?}"),
        }
    }

    #[test]
    fn parse_fallback_when_no_markers() {
        let raw = "Just some prose with no markers at all.";
        match parse_reply(raw) {
            ParsedReply::Fallback(p) => assert_eq!(p, raw),
            other => panic!("expected fallback, got {other:?}"),
        }
    }

    #[test]
    fn blank_title_asks_for_book_title_and_parses_it() {
        let store = tmp_store();
        let book = store.create("", "English", "", true).unwrap();
        let p = chapter_prompt(&book, 1, "", false, None, false);
        assert!(p.contains("BOOK-TITLE:"));
        assert_eq!(
            book_title_from_bible("BOOK-TITLE: The Salt Road\nCAST: A"),
            Some("The Salt Road".to_string())
        );
        assert_eq!(book_title_from_bible("CAST: A"), None);
    }

    #[test]
    fn conclude_flag_injects_the_directive() {
        let store = tmp_store();
        let book = store.create("T", "English", "a heist", false).unwrap();
        let p = chapter_prompt(&book, 3, "wrap it up", false, None, true);
        assert!(p.contains(CONCLUDE_DIRECTIVE));
        assert!(p.contains("LAST"));
        // The normal arc guidance is replaced, not doubled.
        assert!(!p.contains("Judge your position in the arc"));
        let p2 = chapter_prompt(&book, 3, "keep going", false, None, false);
        assert!(!p2.contains(CONCLUDE_DIRECTIVE));
    }

    #[test]
    fn rewrite_prompt_notes_downstream() {
        let store = tmp_store();
        let mut book = store.create("T", "English", "", false).unwrap();
        book.write_chapter(1, "One", "prose", "").unwrap();
        book.write_chapter(2, "Two", "prose two", "").unwrap();
        let p = rewrite_prompt(&book, 1, "kill the dog earlier");
        assert!(p.contains("REWRITE chapter 1"));
        assert!(p.contains("come AFTER"));
        assert!(p.contains("kill the dog earlier"));
    }

    fn tmp_store() -> super::super::store::BookStore {
        let root = std::env::temp_dir().join(format!(
            "bookley-prompt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        super::super::store::BookStore::new(root)
    }
}
