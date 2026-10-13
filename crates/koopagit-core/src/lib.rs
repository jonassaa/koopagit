//! Git/domain core for KoopaGit.
//!
//! This crate has no GUI or OS-integration dependencies. Mutating operations
//! shell out to the `git` CLI (see [`git_cli`]) with the same arguments
//! TortoiseGit builds; read-only queries (discovery, status, log) use `gix`.

pub mod blame;
pub mod commands;
pub mod commit;
pub mod conflict;
pub mod diff;
pub mod error;
pub mod extras;
pub mod fileops;
pub mod git_cli;
pub mod gitconfig;
pub mod history;
pub mod launcher;
pub mod listops;
pub mod log;
pub mod mergeview;
pub mod misccmds;
pub mod rebase;
pub mod remote;
pub mod repo;
pub mod settings;
pub mod state;
pub mod status;
pub mod tools;
// Worker E: remaining commands.
pub mod bugtraq;
pub mod hooks;
pub mod revgraph;

pub use error::{Error, Result};
pub use repo::Repo;
