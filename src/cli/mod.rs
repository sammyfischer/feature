//! Defines the main cli structure, most simple commands, and several helper functions and macros.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::App;

mod base;
mod commit;
mod config_command;
mod graph;
mod list;
mod log;
mod prune;
mod push;
mod start;
mod status;
mod sync;
mod update;

/// Slightly shorter way to get a string from bytes
#[macro_export]
macro_rules! lossy {
  ($bytes:expr) => {
    String::from_utf8_lossy($bytes)
  };
}

/// Waits on the child process, returns result
#[macro_export]
macro_rules! await_child {
  ($child:expr, $name:expr) => {
    match $child.wait() {
      Ok(status) if status.success() => Ok(()),
      Ok(status) => Err(anyhow::anyhow!(
        "{} exited with nonzero exit code: {}",
        $name,
        status
      )),
      Err(e) => Err(anyhow::anyhow!(e)),
    }
  };
}

/// Spawns a git command, passing this macros args as command line args
#[macro_export]
macro_rules! git {
  ($($arg:expr),* $(,)?) => {
    {
      let mut cmd = std::process::Command::new("git");
      $(
        cmd.arg($arg);
      )*
      cmd
    }
  };
}

#[derive(Debug, Parser)]
pub struct Args {
  /// Path to a project-level config file to use
  #[arg(long)]
  pub config: Option<PathBuf>,

  /// Path to a git directory to use
  #[arg(long)]
  pub git_dir: Option<PathBuf>,

  /// Path to a git worktree to use. "work-tree" is an invisible alias in case anyone is used to
  /// git's option with the same spelling
  #[arg(long, visible_alias = "wt", alias = "work-tree", requires = "git_dir")]
  pub worktree: Option<PathBuf>,

  #[command(subcommand)]
  pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
  // ==== FEATURE BRANCH WORKFLOW / SINGLE BRANCH ACTIONS ====
  Start(start::Args),
  Commit(commit::Args),
  Base(base::Args),
  Update(update::Args),
  Push(push::Args),

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  Sync(sync::Args),
  Prune(prune::Args),

  // ==== DISPLAY / INFO ====
  Status(status::Args),
  List(list::Args),
  Log(log::Args),
  Graph(graph::Args),

  // ==== META / FEATURE COMMANDS ====
  Config(config_command::Args),
}

pub fn run(state: App) -> anyhow::Result<()> {
  match &state.command {
    Command::Start(args) => args.run(&state),
    Command::Commit(args) => args.run(&state),
    Command::Base(args) => args.run(&state),
    Command::Update(args) => args.run(&state),
    Command::Push(args) => args.run(&state),
    Command::Sync(args) => args.run(&state),
    Command::Prune(args) => args.run(&state),
    Command::Status(args) => args.run(&state),
    Command::List(args) => args.run(&state),
    Command::Log(args) => args.run(&state),
    Command::Graph(args) => args.run(&state),
    Command::Config(args) => args.run(),
  }
}
