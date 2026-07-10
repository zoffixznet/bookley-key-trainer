//! XDG directory resolution for config, data (books), and stats. Honors an override env
//! `BOOKLEY_DATA_DIR` so tests and the smoke run can use a throwaway location.

use directories::ProjectDirs;
use std::path::PathBuf;

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "Bookley", "BookleyKeyTrainer")
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
