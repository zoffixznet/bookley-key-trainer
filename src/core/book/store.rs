//! On-disk book store. One directory per book:
//!   <books>/<slug>/book.toml         metadata + chapter list + typed progress
//!   <books>/<slug>/bible.md          continuity bible (agent-maintained)
//!   <books>/<slug>/chapters/NN.md    each chapter's Markdown
//!
//! Everything is human-inspectable and survives session loss.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Per-chapter metadata and typed-progress state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterMeta {
    pub n: usize,
    pub file: String,
    pub title: String,
    /// Number of characters typed so far (for resuming a chapter's typing).
    #[serde(default)]
    pub typed_chars: usize,
    /// Whether the user has finished typing this chapter.
    #[serde(default)]
    pub done: bool,
    /// Word count of the chapter prose (informational).
    #[serde(default)]
    pub words: usize,
}

/// Book metadata persisted in book.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMeta {
    pub slug: String,
    pub title: String,
    pub language: String,
    #[serde(default)]
    pub premise: String,
    #[serde(default)]
    pub created: String,
    /// Claude session id for multi-turn continuity, captured from chapter 1.
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub chapters: Vec<ChapterMeta>,
    /// Whether the user confirmed a fully-AI-invented book (no clarifying questions).
    #[serde(default)]
    pub ai_invent_confirmed: bool,
    /// Whether the story has been concluded by the agent.
    #[serde(default)]
    pub concluded: bool,
}

/// A loaded book with a handle to its directory.
#[derive(Debug, Clone)]
pub struct Book {
    pub meta: BookMeta,
    pub dir: PathBuf,
}

impl Book {
    fn meta_path(&self) -> PathBuf {
        self.dir.join("book.toml")
    }
    pub fn bible_path(&self) -> PathBuf {
        self.dir.join("bible.md")
    }
    pub fn chapters_dir(&self) -> PathBuf {
        self.dir.join("chapters")
    }
    pub fn chapter_path(&self, n: usize) -> PathBuf {
        self.chapters_dir().join(format!("{n:02}.md"))
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::create_dir_all(self.chapters_dir())?;
        let s = toml::to_string_pretty(&self.meta).map_err(std::io::Error::other)?;
        std::fs::write(self.meta_path(), s)
    }

    pub fn read_bible(&self) -> String {
        std::fs::read_to_string(self.bible_path()).unwrap_or_default()
    }

    pub fn write_bible(&self, bible: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.bible_path(), bible)
    }

    pub fn read_chapter(&self, n: usize) -> std::io::Result<String> {
        std::fs::read_to_string(self.chapter_path(n))
    }

    /// The count of chapters generated.
    pub fn chapter_count(&self) -> usize {
        self.meta.chapters.len()
    }

    /// Are all generated chapters fully typed? Gate for generating the next chapter.
    pub fn all_chapters_typed(&self) -> bool {
        self.meta.chapters.iter().all(|c| c.done)
    }

    /// The tail of the previous chapter (last ~800 chars) for continuity context.
    pub fn previous_chapter_tail(&self, upto_n: usize) -> String {
        if upto_n <= 1 {
            return String::new();
        }
        let prev = upto_n - 1;
        match self.read_chapter(prev) {
            Ok(text) => {
                let chars: Vec<char> = text.chars().collect();
                let start = chars.len().saturating_sub(800);
                chars[start..].iter().collect()
            }
            Err(_) => String::new(),
        }
    }

    /// Add or replace a chapter's prose and bible on disk, updating metadata. `n` is 1-based.
    pub fn write_chapter(
        &mut self,
        n: usize,
        title: &str,
        prose: &str,
        bible: &str,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(self.chapters_dir())?;
        std::fs::write(self.chapter_path(n), prose)?;
        if !bible.trim().is_empty() {
            self.write_bible(bible)?;
        }
        let words = prose.split_whitespace().count();
        let file = format!("chapters/{n:02}.md");
        if let Some(existing) = self.meta.chapters.iter_mut().find(|c| c.n == n) {
            // Rewrite: reset typing progress.
            existing.title = title.to_string();
            existing.file = file;
            existing.typed_chars = 0;
            existing.done = false;
            existing.words = words;
        } else {
            self.meta.chapters.push(ChapterMeta {
                n,
                file,
                title: title.to_string(),
                typed_chars: 0,
                done: false,
                words,
            });
            self.meta.chapters.sort_by_key(|c| c.n);
        }
        self.save()
    }

    /// Mark a chapter's typed progress.
    pub fn set_typed_progress(&mut self, n: usize, typed_chars: usize, done: bool) {
        if let Some(c) = self.meta.chapters.iter_mut().find(|c| c.n == n) {
            c.typed_chars = typed_chars;
            c.done = done;
        }
        let _ = self.save();
    }

    /// Concatenate all chapters into a single export Markdown with a title page.
    pub fn export_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", display_title(&self.meta)));
        if !self.meta.language.is_empty() {
            out.push_str(&format!("*Language: {}*\n\n", self.meta.language));
        }
        if !self.meta.premise.trim().is_empty() {
            out.push_str(&format!("> {}\n\n", self.meta.premise.trim()));
        }
        out.push_str("---\n\n");
        for c in &self.meta.chapters {
            let prose = self.read_chapter(c.n).unwrap_or_default();
            let heading = if c.title.trim().is_empty() {
                format!("Chapter {}", c.n)
            } else {
                format!("Chapter {}: {}", c.n, c.title.trim())
            };
            out.push_str(&format!("## {heading}\n\n"));
            out.push_str(prose.trim());
            out.push_str("\n\n");
        }
        out
    }
}

pub fn display_title(meta: &BookMeta) -> String {
    if meta.title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        meta.title.trim().to_string()
    }
}

/// Slugify a title into a filesystem-safe directory name.
pub fn slugify(title: &str) -> String {
    let mut slug: String = title
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        format!("untitled-{}", short_id())
    } else {
        slug
    }
}

fn short_id() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", n & 0xffffff)
}

/// The book store rooted at a directory.
pub struct BookStore {
    pub root: PathBuf,
}

impl BookStore {
    pub fn new(root: PathBuf) -> Self {
        BookStore { root }
    }

    /// Create a new book with a unique slug and persist its initial metadata.
    pub fn create(
        &self,
        title: &str,
        language: &str,
        premise: &str,
        ai_invent_confirmed: bool,
    ) -> std::io::Result<Book> {
        std::fs::create_dir_all(&self.root)?;
        let base = slugify(title);
        let mut slug = base.clone();
        let mut i = 2;
        while self.root.join(&slug).exists() {
            slug = format!("{base}-{i}");
            i += 1;
        }
        let dir = self.root.join(&slug);
        let meta = BookMeta {
            slug: slug.clone(),
            title: title.trim().to_string(),
            language: language.trim().to_string(),
            premise: premise.trim().to_string(),
            created: super::super::metrics::now_iso(),
            session_id: None,
            chapters: Vec::new(),
            ai_invent_confirmed,
            concluded: false,
        };
        let book = Book { meta, dir };
        book.save()?;
        Ok(book)
    }

    /// Load a single book by slug.
    pub fn load(&self, slug: &str) -> std::io::Result<Book> {
        let dir = self.root.join(slug);
        let meta_str = std::fs::read_to_string(dir.join("book.toml"))?;
        let meta: BookMeta = toml::from_str(&meta_str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Book { meta, dir })
    }

    /// List all books, skipping corrupt entries (logged, not fatal).
    pub fn list(&self) -> Vec<Book> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let slug = match path.file_name().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            match self.load(&slug) {
                Ok(b) => out.push(b),
                Err(e) => tracing::warn!("skipping corrupt book {slug}: {e}"),
            }
        }
        out.sort_by_key(|a| a.meta.title.to_lowercase());
        out
    }

    pub fn delete(&self, slug: &str) -> std::io::Result<()> {
        let dir = self.root.join(slug);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Convenience: default store rooted at the XDG books dir.
pub fn default_store() -> BookStore {
    let root = super::super::paths::books_dir().unwrap_or_else(|| PathBuf::from("books"));
    BookStore::new(root)
}

#[allow(dead_code)]
fn _assert_send(_p: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "bookley-books-{}-{}",
            std::process::id(),
            super::short_id()
        ))
    }

    #[test]
    fn create_add_rewrite_reload_roundtrip() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        let mut book = store
            .create(
                "The Long Night",
                "English",
                "A city that never sleeps.",
                false,
            )
            .unwrap();
        book.write_chapter(1, "Arrival", "It began at dusk.", "CAST: Mara")
            .unwrap();
        book.set_typed_progress(1, 17, true);
        assert!(book.all_chapters_typed());

        // Reload from disk.
        let reloaded = store.load(&book.meta.slug).unwrap();
        assert_eq!(reloaded.meta.title, "The Long Night");
        assert_eq!(reloaded.chapter_count(), 1);
        assert!(reloaded.meta.chapters[0].done);
        assert_eq!(reloaded.read_chapter(1).unwrap(), "It began at dusk.");
        assert_eq!(reloaded.read_bible(), "CAST: Mara");

        // Rewrite chapter 1 resets typed progress.
        let mut reloaded = reloaded;
        reloaded
            .write_chapter(
                1,
                "Arrival (v2)",
                "It began at midnight.",
                "CAST: Mara, Doss",
            )
            .unwrap();
        assert!(!reloaded.meta.chapters[0].done);
        assert_eq!(reloaded.meta.chapters[0].typed_chars, 0);
        assert_eq!(reloaded.read_chapter(1).unwrap(), "It began at midnight.");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn export_markdown_has_title_and_chapters() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        let mut book = store.create("Salt", "English", "", false).unwrap();
        book.write_chapter(1, "One", "Prose one.", "").unwrap();
        book.write_chapter(2, "Two", "Prose two.", "").unwrap();
        let md = book.export_markdown();
        assert!(md.starts_with("# Salt"));
        assert!(md.contains("## Chapter 1: One"));
        assert!(md.contains("Prose two."));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn blank_title_slug_is_unique_and_nonempty() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        let b1 = store.create("", "English", "", true).unwrap();
        let b2 = store.create("", "English", "", true).unwrap();
        assert!(!b1.meta.slug.is_empty());
        assert_ne!(b1.meta.slug, b2.meta.slug);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_skips_corrupt_book() {
        let root = tmp_root();
        let store = BookStore::new(root.clone());
        store.create("Good", "English", "", false).unwrap();
        // Create a corrupt book dir.
        let bad = root.join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("book.toml"), "not = valid = toml =").unwrap();
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].meta.title, "Good");
        let _ = std::fs::remove_dir_all(&root);
    }
}
