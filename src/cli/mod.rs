//! Defines the main cli structure, most simple commands, and several helper functions and macros.

use clap::{Parser, Subcommand};

use crate::config::Config;

mod base;
mod commit;
mod config_cmd;
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

/// Automatically opens a suitable git repo. Panics if it can't find one.
#[macro_export]
macro_rules! open_repo {
  () => {
    git2::Repository::open_from_env().expect("Failed to open git repo")
  };
}

#[derive(Debug, Parser)]
pub struct Args {
  #[command(subcommand)]
  pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
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
  /// Interact with feature config
  Config {
    #[command(subcommand)]
    args: config_cmd::Args,
  },
}

pub struct Cli {
  pub(crate) config: Config,
  pub(crate) args: Args,
}

impl Cli {
  pub fn new() -> Self {
    let config = crate::config::load().unwrap_or_default();
    let args = Args::parse();
    Self { config, args }
  }

  pub fn run(&mut self) -> anyhow::Result<()> {
    match &self.args.action {
      Action::Start(args) => args.run(self),
      Action::Commit(args) => args.run(),
      Action::Base(args) => args.run(),
      Action::Update(args) => args.run(),
      Action::Push(args) => args.run(self),
      Action::Sync(args) => args.run(self),
      Action::Prune(args) => args.run(self),
      Action::Status(args) => args.run(self),
      Action::List(args) => args.run(),
      Action::Log(args) => args.run(self),
      Action::Graph(args) => args.run(self),
      Action::Config { args } => args.run(),
    }
  }
}
