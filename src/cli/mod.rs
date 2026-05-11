//! Defines the main cli structure, most simple commands, and several helper functions and macros.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueHint};

use crate::App;

mod base;
mod check;
mod commit;
mod complete;
mod completions;
mod config_command;
mod end;
mod graph;
mod list;
mod prune;
mod push;
mod show;
mod start;
mod status;
mod sync;
mod update;

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
#[command(
  long_version = env!("CARGO_PKG_VERSION"),
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Path to a project-level config file to use
  #[arg(long, value_hint = ValueHint::FilePath)]
  pub config: Option<PathBuf>,

  /// Path to a git directory to use
  #[arg(long, value_hint = ValueHint::DirPath)]
  pub git_dir: Option<PathBuf>,

  /// Path to a git worktree to use
  #[arg(long, visible_alias = "wt", requires = "git_dir", value_hint = ValueHint::DirPath)]
  pub work_tree: Option<PathBuf>,

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
  Check(check::Args),
  End(end::Args),

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  Sync(sync::Args),
  Prune(prune::Args),

  // ==== DISPLAY / INFO ====
  Status(status::Args),
  List(list::Args),
  Graph(graph::Args),
  Show(show::Args),

  // ==== META / FEATURE COMMANDS ====
  Config(config_command::Args),
  Completions(completions::Args),
  Complete(complete::Args),
}

pub fn run(state: App) -> anyhow::Result<()> {
  match &state.command {
    Command::Start(args) => args.run(&state),
    Command::Commit(args) => args.run(&state),
    Command::Base(args) => args.run(&state),
    Command::Update(args) => args.run(&state),
    Command::Push(args) => args.run(&state),
    Command::Check(args) => args.run(&state),
    Command::End(args) => args.run(&state),
    Command::Sync(args) => args.run(&state),
    Command::Prune(args) => args.run(&state),
    Command::Status(args) => args.run(&state),
    Command::List(args) => args.run(&state),
    Command::Graph(args) => args.run(&state),
    Command::Show(args) => args.run(&state),
    Command::Config(args) => args.run(&state.config),
    Command::Completions(args) => args.run(),
    Command::Complete(args) => args.run(&state),
  }
}
