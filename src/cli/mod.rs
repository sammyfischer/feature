//! CLI frontend for feature.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueHint};

use crate::App;
use crate::cli::base::BaseArgs;
use crate::cli::branch_list::BranchListArgs;
use crate::cli::check::CheckArgs;
use crate::cli::commit::CommitArgs;
use crate::cli::complete::CompleteArgs;
use crate::cli::completions::CompletionsArgs;
use crate::cli::config::ConfigArgs;
use crate::cli::end::EndArgs;
use crate::cli::project::ProjectArgs;
use crate::cli::protect::ProtectArgs;
use crate::cli::prune::PruneArgs;
use crate::cli::push::PushArgs;
use crate::cli::show::ShowArgs;
use crate::cli::start::StartArgs;
use crate::cli::status::StatusArgs;
use crate::cli::sync::SyncArgs;
use crate::cli::update::UpdateArgs;
use crate::cli::version::VersionArgs;
use crate::cli::version_list::VersionListArgs;
use crate::cli::version_log::VersionLogArgs;
use crate::cli::wip::WipArgs;

mod advice;
mod base;
mod branch_list;
mod check;
mod commit;
mod complete;
mod completions;
mod config;
mod display;
mod end;
mod project;
mod protect;
mod prune;
mod push;
mod show;
mod start;
pub mod status;
mod sync;
mod term;
mod update;
mod version;
mod version_list;
mod version_log;
mod wip;

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
  Start(StartArgs),
  Commit(CommitArgs),
  Base(BaseArgs),
  Update(UpdateArgs),
  Push(PushArgs),
  Check(CheckArgs),
  End(EndArgs),
  Wip(WipArgs),

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  Sync(SyncArgs),
  Prune(PruneArgs),
  Protect(ProtectArgs),
  Version(VersionArgs),
  Project(ProjectArgs),

  // ==== DISPLAY / INFO ====
  Status(StatusArgs),
  BranchList(BranchListArgs),
  VersionList(VersionListArgs),
  VersionLog(VersionLogArgs),
  Show(ShowArgs),

  // ==== META / FEATURE COMMANDS ====
  Config(ConfigArgs),
  Completions(CompletionsArgs),
  Complete(CompleteArgs),
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
    Command::Wip(args) => args.run(&state),

    Command::Sync(args) => args.run(&state),
    Command::Prune(args) => args.run(&state),
    Command::Protect(args) => args.run(&state),
    Command::Version(args) => args.run(&state),
    Command::Project(args) => args.run(&state),

    Command::Status(args) => args.run(&state),
    Command::BranchList(args) => args.run(&state),
    Command::VersionList(args) => args.run(&state),
    Command::VersionLog(args) => args.run(&state),
    Command::Show(args) => args.run(&state),

    Command::Config(args) => args.run(&state.config),
    Command::Completions(args) => args.run(),
    Command::Complete(args) => args.run(&state),
  }
}
