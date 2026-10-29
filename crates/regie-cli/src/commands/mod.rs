//! The subcommands of the `regie` tool.
//!
//! Every subcommand is a small function that takes its arguments, does the
//! work and returns the exit code. The argument parsing itself lives in
//! `main.rs`, so a subcommand never has to look at the raw command line.

pub mod artifact;
pub mod intent;
pub mod matching;
pub mod search;
pub mod serve;
pub mod show;
