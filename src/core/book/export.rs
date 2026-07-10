//! Export: turn a book into (a) clean plain text for typing, (b) a single Markdown file,
//! and (c) a PDF via an in-process Typst engine with bundled fonts.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use super::store::Book;

/// Convert chapter Markdown into clean plain text for the typing target: strip Markdown
/// syntax, collapse to readable prose, keep paragraph breaks as blank lines.
pub fn markdown_to_plain(md: &str) -> String {
    let parser = Parser::new(md);
    let mut out = String::new();
    let mut list_depth = 0usize;
    for event in parser {
        match event {
            Event::Text(t) => out.push_str(&t),
            Event::Code(t) => out.push_str(&t),
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push('\n'),
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => out.push_str("\n\n"),
            Event::Start(Tag::Heading { .. }) => {}
            Event::End(TagEnd::Heading(_)) => out.push_str("\n\n"),
            Event::Start(Tag::Item) => {
                for _ in 0..list_depth {
                    out.push_str("  ");
                }
                out.push_str("- ");
            }
            Event::End(TagEnd::Item) => out.push('\n'),
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                out.push('\n');
            }
            Event::Start(Tag::BlockQuote(_)) => {}
            Event::End(TagEnd::BlockQuote(_)) => out.push_str("\n\n"),
            _ => {}
        }
    }
    // Normalize excessive blank lines and trim.
    let mut cleaned = String::new();
    let mut blanks = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 1 {
                cleaned.push('\n');
            }
        } else {
            blanks = 0;
            cleaned.push_str(line.trim_end());
            cleaned.push('\n');
        }
    }
    cleaned.trim().to_string()
}

/// The single-file Markdown export of the whole book.
pub fn export_markdown(book: &Book) -> String {
    book.export_markdown()
}

// The PDF carries book content only: an optional cover page, the title page, and the
// chapters. Generation inputs (premise, language, continuation history) never appear.
const PDF_TEMPLATE: &str = r##"#import sys: inputs

#set document(title: inputs.title, author: "Bookley Key Trainer")
#set text(size: 11pt, lang: inputs.lang)
#set par(justify: true, leading: 0.72em, first-line-indent: 1.2em)

// Cover: page one, full bleed, before everything else.
#if inputs.cover != none {
  set page(paper: "a5", margin: 0pt)
  place(top + left, image(inputs.cover, width: 100%, height: 100%, fit: "cover"))
}

#set page(paper: "a5", margin: (x: 2.2cm, y: 2.4cm), numbering: "1")
#counter(page).update(1)

// Title page
#align(center + horizon)[
  #text(size: 26pt, weight: "bold")[#inputs.title]
]
#pagebreak()

#for ch in inputs.chapters [
  #heading(level: 1, numbering: none)[#ch.title]
  #v(0.4em)
  #for para in ch.paras [
    #para
    #parbreak()
  ]
  #pagebreak(weak: true)
]
"##;

/// Map the book's free-text language name to an ISO 639-1 code for Typst's `lang`
/// setting (hyphenation, justification, quotes). Unknown languages fall back to "en";
/// two-letter inputs are trusted as codes already.
fn lang_code(language: &str) -> String {
    let lang = language.trim().to_lowercase();
    let code = match lang.as_str() {
        "english" => "en",
        "arabic" => "ar",
        "chinese" | "mandarin" => "zh",
        "czech" => "cs",
        "danish" => "da",
        "dutch" => "nl",
        "esperanto" => "eo",
        "finnish" => "fi",
        "french" => "fr",
        "german" => "de",
        "greek" => "el",
        "hebrew" => "he",
        "hindi" => "hi",
        "hungarian" => "hu",
        "italian" => "it",
        "japanese" => "ja",
        "korean" => "ko",
        "norwegian" => "no",
        "polish" => "pl",
        "portuguese" => "pt",
        "romanian" => "ro",
        "russian" => "ru",
        "spanish" => "es",
        "swedish" => "sv",
        "turkish" => "tr",
        "ukrainian" => "uk",
        _ => {
            if lang.len() == 2 && lang.chars().all(|c| c.is_ascii_lowercase()) {
                return lang;
            }
            "en"
        }
    };
    code.to_string()
}

/// Generate a PDF for `book`. Returns the PDF bytes. Errors are returned as strings so the
/// UI can show a message instead of panicking.
pub fn export_pdf(book: &Book) -> Result<Vec<u8>, String> {
    use typst::foundations::{Dict, IntoValue, Value};
    use typst_as_lib::typst_kit_options::TypstKitFontOptions;
    use typst_as_lib::TypstEngine;

    let title = super::store::display_title(&book.meta);
    // Cover page (optional): the rasterized cover PNG stored with the book.
    let cover_value = match std::fs::read(book.cover_path()) {
        Ok(bytes) => typst::foundations::Bytes::new(bytes).into_value(),
        Err(_) => Value::None,
    };

    // Build chapter dicts: title + list of paragraph strings (from cleaned plain text).
    let mut chapters: Vec<Value> = Vec::new();
    for c in &book.meta.chapters {
        let md = book.read_chapter(c.n).unwrap_or_default();
        let plain = markdown_to_plain(&md);
        let paras: Vec<Value> = plain
            .split("\n\n")
            .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|p| !p.is_empty())
            .map(|p| p.into_value())
            .collect();
        let heading = if c.title.trim().is_empty() {
            format!("Chapter {}", c.n)
        } else {
            format!("Chapter {}: {}", c.n, c.title.trim())
        };
        let mut d = Dict::new();
        d.insert("title".into(), heading.into_value());
        d.insert("paras".into(), paras.into_value());
        chapters.push(d.into_value());
    }

    if chapters.is_empty() {
        return Err("No chapters to export yet.".to_string());
    }

    let mut input = Dict::new();
    input.insert("title".into(), title.into_value());
    input.insert("cover".into(), cover_value);
    input.insert("chapters".into(), chapters.into_value());
    input.insert(
        "lang".into(),
        lang_code(&book.meta.language).to_string().into_value(),
    );

    // Fonts: the embedded set (Libertinus, New Computer Modern, DejaVu) is the
    // deterministic base but covers Latin/Cyrillic/Greek only; system fonts are searched
    // too so a book in e.g. Japanese or Arabic renders with real glyphs (from the user's
    // installed fonts) instead of silent tofu boxes.
    let engine = TypstEngine::builder()
        .main_file(PDF_TEMPLATE)
        .search_fonts_with(
            TypstKitFontOptions::default()
                .include_system_fonts(true)
                .include_embedded_fonts(true),
        )
        .build();

    let compiled = engine.compile_with_input(input);
    let doc = compiled
        .output
        .map_err(|e| format!("Typst compile failed: {e:?}"))?;
    let pdf = typst_pdf::pdf(&doc, &Default::default())
        .map_err(|e| format!("PDF generation failed: {e:?}"))?;
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::book::store::BookStore;
    use std::path::PathBuf;

    fn tmp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "bookley-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ))
    }

    #[test]
    fn markdown_to_plain_strips_syntax() {
        let md = "# Chapter One\n\nShe *walked* to the `door` and stopped.\n\nA new paragraph.";
        let plain = markdown_to_plain(md);
        assert!(!plain.contains('#'));
        assert!(!plain.contains('*'));
        assert!(!plain.contains('`'));
        assert!(plain.contains("She walked to the door and stopped."));
        assert!(plain.contains("A new paragraph."));
    }

    #[test]
    fn pdf_starts_with_magic_and_is_nontrivial() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        let mut book = store
            .create("Salt and Iron", "English", "A blacksmith's secret.", false)
            .unwrap();
        book.write_chapter(
            1,
            "The Forge",
            "The fire caught at dawn.\n\nMara worked the bellows until her arms ached.",
            "",
        )
        .unwrap();
        book.write_chapter(2, "Ash", "By dusk the forge had gone cold.", "")
            .unwrap();
        let pdf = export_pdf(&book).expect("pdf");
        assert!(pdf.starts_with(b"%PDF"), "missing %PDF magic");
        assert!(pdf.len() > 1000, "pdf too small: {}", pdf.len());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stored cover becomes page one of the PDF (the embedded image makes the file
    /// substantially larger), and the title page carries no prompt material.
    #[test]
    fn pdf_embeds_the_cover_as_page_one() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        let mut book = store
            .create("Covered", "Esperanto", "A very secret premise.", false)
            .unwrap();
        book.write_chapter(1, "One", "Some prose.", "").unwrap();
        let plain_pdf = export_pdf(&book).expect("pdf without cover");

        let svg = crate::core::book::cover::fallback_cover_svg("Covered");
        let png = crate::core::book::cover::rasterize_svg_to_png(&svg).expect("cover png");
        std::fs::write(book.cover_path(), &png).unwrap();
        let cover_pdf = export_pdf(&book).expect("pdf with cover");
        assert!(cover_pdf.starts_with(b"%PDF"));
        assert!(
            cover_pdf.len() > plain_pdf.len() + 10_000,
            "cover image must be embedded ({} -> {})",
            plain_pdf.len(),
            cover_pdf.len()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lang_code_maps_names_codes_and_unknowns() {
        assert_eq!(lang_code("English"), "en");
        assert_eq!(lang_code(" Russian "), "ru");
        assert_eq!(lang_code("Japanese"), "ja");
        assert_eq!(lang_code("pt"), "pt", "bare codes pass through");
        assert_eq!(lang_code("Klingon"), "en", "unknowns fall back to en");
        assert_eq!(lang_code(""), "en");
    }

    /// A non-English book compiles with its own language setting and the chapter text
    /// present (Cyrillic is covered by the embedded fonts).
    #[test]
    fn pdf_exports_a_cyrillic_book() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        let mut book = store.create("Ночь", "Russian", "", false).unwrap();
        book.write_chapter(1, "Глава", "Город спал, и только маяк не спал.", "")
            .unwrap();
        let pdf = export_pdf(&book).expect("pdf");
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 1000);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn export_pdf_empty_book_errs() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        let book = store.create("Empty", "English", "", false).unwrap();
        assert!(export_pdf(&book).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
