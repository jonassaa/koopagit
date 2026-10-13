//! Platform-independent shell-integration logic shared by every file-manager
//! extension: selection flags, the context-menu table and overlays.
//!
//! Extensions stay thin: they call into this crate (directly or through the
//! `koopagit-shell` helper binary) and spawn the launcher for actions.

pub mod actions;
pub mod flags;
pub mod json;
pub mod menu;
pub mod overlay;
pub mod pathfile;
pub mod pathfilter;
pub mod selection;
