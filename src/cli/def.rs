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

  /// View git log with pretty settings by default
  Log,

  /// View git graph with pretty settings by default
  Graph(GraphArgs),

  /// Modify config values or initialize a config file
  Config {
    #[command(subcommand)]
    args: ConfigCmd,
  },
}

#[derive(clap::Args, Clone, Debug)]
pub struct GraphArgs {
  #[arg(short = 'i', long = "interactive")]
  pub interactive: Option<bool>,

  #[arg(short = 'p', long = "pager", default_value = "less")]
  pub pager: Option<String>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigCmd {
  Set(ConfigSetArgs),
}

#[derive(clap::Args, Clone, Debug)]
pub struct ConfigSetArgs {
  #[arg(long)]
  pub protected_branches: Option<Vec<String>>,

  #[arg(long)]
  pub interactive: Option<bool>,

  #[arg(long)]
  pub pager: Option<String>,
}

pub struct Cli {
  pub(crate) config: Config,
  pub(crate) args: Args,
}
