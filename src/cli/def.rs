//! Defines the structure of the cli

use clap::{Parser, Subcommand};

use crate::config::Config;

#[derive(Debug, Parser)]
pub struct Args {
  #[command(subcommand)]
  pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
  /// Start a new feature branch
  Start(StartArgs),

  /// Commit using remaining args as commit message
  Commit {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    /// Words to join together as commit message
    words: Vec<String>,
  },

  /// Update the current branch against its base branch
  Update,

  /// Join the current branch into its base branch
  Merge,

  /// Add branch to list of protected branches
  #[command(alias = "prot")]
  Protect { branch: String },

  /// Remove branch from list of protected branches
  #[command(alias = "unprot")]
  Unprotect { branch: String },

  /// Clean up merged branches
  Prune {
    #[arg(long = "dry-run")]
    dry_run: bool,
  },

  /// List branches
  #[command(alias = "ls")]
  List,

  /// View git log with entire commit subject line, followed by author name and relative date
  Log,

  /// View git graph with author name and relative date in noticable colors. Truncates commit
  /// subject to try to fit it in one line
  Graph,

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

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigCmd {
  Set(ConfigSetArgs),
}

#[derive(clap::Args, Clone, Debug)]
pub struct ConfigSetArgs {
  #[arg(long = "branch-sep", aliases = ["branch_sep"])]
  pub branch_sep: Option<String>,
}

pub struct Cli {
  pub(crate) config: Config,
  pub(crate) args: Args,
}
