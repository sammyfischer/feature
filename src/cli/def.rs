//! Defines the structure of the cli

use clap::{Parser, Subcommand};

use crate::{cli::config::ConfigCmd, config::Config};

#[derive(Debug, Parser)]
pub struct Args {
  #[command(subcommand)]
  pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
  // ==== FEATURE BRANCH WORKFLOW / SINGLE BRANCH ACTIONS ====
  /// Start a new feature branch
  Start(StartArgs),

  /// Commit using remaining args as commit message
  Commit {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    /// Words to join together as commit message
    words: Vec<String>,
  },

  /// Update the current branch against its base branch
  Update {
    /// The name of the base branch to use. Defaults to the repo's main base
    base: Option<String>,
  },

  /// Push current branch to remote
  Push,

  /// Add branch to list of protected branches
  #[command(visible_alias = "prot")]
  Protect { branch: String },

  /// Remove branch from list of protected branches
  #[command(visible_alias = "unprot")]
  Unprotect { branch: String },

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  /// Syncs entire repo with all remotes
  Sync,

  /// Clean up merged branches
  Prune {
    #[arg(long = "dry-run")]
    dry_run: bool,
  },

  // ==== DISPLAY / INFO ====
  /// List branches
  #[command(visible_alias = "ls")]
  List,

  /// View git log with entire commit subject line, followed by author name and relative date
  Log,

  /// View git graph with author name and relative date in noticable colors. Truncates commit
  /// subject to try to fit it in one line
  Graph,

  // ==== META / FEATURE COMMANDS ====
  /// Modify config values or initialize a config file
  Config {
    #[command(subcommand)]
    args: ConfigCmd,
  },
}

#[derive(clap::Args, Clone, Debug)]
pub struct StartArgs {
  #[arg(long, default_value = "-")]
  /// The separator to use when joining words
  pub sep: Option<String>,

  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
  /// Words to join together as branch name
  pub words: Vec<String>,
}

pub struct Cli {
  pub(crate) config: Config,
  pub(crate) args: Args,
}
