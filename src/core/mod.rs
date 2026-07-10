//! Pure-logic core: no egui dependencies except the `egui::Key` enum (a small value type
//! used as the canonical key identity). Everything here is unit-testable.

pub mod book;
pub mod config;
pub mod keys;
pub mod metrics;
pub mod paths;
pub mod session;
pub mod stats_store;
pub mod text_source;
pub mod wordlist;
