//! XDG directory resolution for config, data (books), and stats. Honors an override env
//! `BOOKLEY_DATA_DIR` so tests and the smoke run can use a throwaway location.

use directories::ProjectDirs;
use std::path::PathBuf;

fn project_dirs() -> Option<ProjectDirs> {
    // Yields ~/.config/bookley-key-trainer and ~/.local/share/bookley-key-trainer on
    // Linux: the app's on-disk names all use the full "bookley-key-trainer".
    ProjectDirs::from("dev", "Bookley", "bookley-key-trainer")
}

/// One-time migration: early builds stored config and data under the legacy
/// "bookleykeytrainer" project dirs. Rename them to the full "bookley-key-trainer"
/// names when the new dirs do not exist yet. The rename moves the books, settings,
/// stats, and stored token in one atomic same-filesystem operation; on any failure the
/// legacy dirs are left untouched (never delete, never overwrite).
pub fn migrate_legacy_dirs() {
    if std::env::var("BOOKLEY_DATA_DIR").is_ok() {
        return; // overridden location (tests/smoke): nothing to migrate
    }
    let (Some(old), Some(new)) = (
        ProjectDirs::from("dev", "Bookley", "BookleyKeyTrainer"),
        project_dirs(),
    ) else {
        return;
    };
    for (from, to) in [
        (old.config_dir(), new.config_dir()),
        (old.data_dir(), new.data_dir()),
    ] {
        if from.exists() && !to.exists() {
            if let Some(parent) = to.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::rename(from, to) {
                Ok(()) => {
                    tracing::info!("migrated {} -> {}", from.display(), to.display());
                }
                Err(e) => {
                    tracing::error!(
                        "could not migrate {} to {}: {e}; the legacy dir stays in place",
                        from.display(),
                        to.display()
                    );
                }
            }
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BOOKLEY_DATA_DIR") {
        return Some(PathBuf::from(p).join("config"));
    }
    project_dirs().map(|d| d.config_dir().to_path_buf())
}

pub fn data_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BOOKLEY_DATA_DIR") {
        return Some(PathBuf::from(p).join("data"));
    }
    project_dirs().map(|d| d.data_dir().to_path_buf())
}

pub fn books_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("books"))
}

pub fn stats_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("stats.json"))
}

/// Where the bundled novelist plugin is staged so `--plugin-dir` works from an installed
/// binary. We copy the compiled-in skill files here on first use.
pub fn plugin_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("plugin"))
}
