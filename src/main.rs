#![feature(trim_prefix_suffix)]

use clap::{Parser, Subcommand};

use crate::cli_error::{CliError, CliResult};
use crate::commands::{commit, list, log, merge, prune, start, update};

mod cli_error;
mod commands;

#[derive(Parser, Debug)]
struct Args {
  #[command(subcommand)]
  action: Action,
}

#[derive(Subcommand, Debug)]
enum Action {
  /// Start a new feature branch
  Start {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    /// Words to join together as branch name
    words: Vec<String>,
  },

  Commit {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    /// Words to join together as commit message
    words: Vec<String>,
  },

  /// Update the current branch against its base branch
  Update,

  /// Join the current branch into its base branch
  Merge,

  /// Clean up merged branches
  Prune,

  /// List branches
  List,

  /// View git log with pretty settings by default
  Log,
}

fn main() -> CliResult {
  let args = Args::parse();

  match args.action {
    Action::Start { words } => start(&words),
    Action::Commit { words } => commit(&words),
    Action::Update => update(),
    Action::Merge => merge(),
    Action::Prune => prune(),
    Action::List => list(),
    Action::Log => log(),
  }
}

/// Checks if a branch name is allowed. This is likely more strict than actual git rules for branch
/// names.
fn validate_branch_name(name: &str) -> CliResult {
  if name.contains(|c: char| !(c.is_alphanumeric() || c == '/' || c == '-')) {
    Err(CliError::BadBranchName)
  } else {
    Ok(())
  }
}
