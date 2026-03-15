//! Defines the main cli structure, most simple commands, and several helper functions and macros.

use clap::{Parser, Subcommand};
use dialoguer::Confirm;

use crate::cli::error::CliError;
use crate::config::Config;

mod commit;
mod config_cmd;
mod db_cmd;
pub mod error;
mod graph;
mod prune;
mod push;
mod start;
mod sync;
mod update;

/// Waits on the child process, returns result
#[macro_export]
macro_rules! await_child {
  ($child:expr, $msg:expr) => {
    if $child.wait().is_ok_and(|status| status.success()) {
      Ok(())
    } else {
      Err($crate::cli::error::CliError::SubprocessFailed(
        $msg.to_string(),
      ))
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

pub type CliResult<T = ()> = Result<T, CliError>;

#[derive(Debug, Parser)]
pub struct Args {
  #[command(subcommand)]
  pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
  // ==== FEATURE BRANCH WORKFLOW / SINGLE BRANCH ACTIONS ====
  /// Start a new feature branch
  Start(start::Args),

  /// Commit using remaining args as commit message
  Commit(commit::Args),

  /// Update the current branch against its base branch
  Update(update::Args),

  /// Push current branch to remote
  Push(push::Args),

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  /// Syncs all base (protected) branches with remotes. Only fast-forwards branches, refuses to
  /// rebase/merge
  Sync,

  /// Clean up merged branches. A branch is merged if all its commits are found on its base or the
  /// trunk
  Prune(prune::Args),

  // ==== DISPLAY / INFO ====
  /// List branches
  #[command(visible_alias = "ls")]
  List,

  /// View git log with entire commit subject line, followed by author name and relative date
  Log,

  /// View git graph with author name and relative date in noticable colors. Truncates each line to
  /// the terminal width
  Graph,

  // ==== META / FEATURE COMMANDS ====
  /// Interact with feature config
  Config {
    #[command(subcommand)]
    args: config_cmd::Args,
  },

  /// Interact with feature database
  Db {
    #[command(subcommand)]
    args: db_cmd::Args,
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

  pub fn run(&mut self) -> CliResult {
    match &self.args.action {
      Action::Start(args) => args.run(self),
      Action::Commit(args) => args.run(),
      Action::Update(args) => args.run(),
      Action::Push(args) => args.run(self),
      Action::Sync => sync::sync(self),
      Action::Prune(args) => args.run(self),
      Action::List => self.list(),
      Action::Log => self.log(),
      Action::Graph => graph::graph(),
      Action::Config { args } => args.run(),
      Action::Db { args } => args.run(),
    }
  }

  fn list(&self) -> CliResult {
    await_child!(git!("branch", "-vv").spawn()?, "Failed to call git")
  }

  fn log(&self) -> CliResult {
    // git pretty format:
    // %h = hash, %d = decorator (e.g. branch pointing to that commit)
    // %s = subject (commit description title line)
    // %an = author name, %ar = author date (relative)
    await_child!(
      git!(
        "log",
        "--all",
        "--pretty=format:%C(auto)%h%d %C(reset)%s %C(dim)(%an, %ar)"
      )
      .spawn()?,
      "Failed to call git"
    )
  }
}

/// Gets current branch via `git branch --show-current`
fn get_current_branch() -> CliResult<String> {
  let output = git!("branch", "--show-current").output()?;
  let output = String::from_utf8(output.stdout)?;
  Ok(output.trim().to_string())
}

/// Returns a list of all local branches, or None if there was an error getting output
fn get_all_branches() -> CliResult<Vec<String>> {
  let output = git!("branch", "--format=%(refname:short)").output()?;
  let output = String::from_utf8(output.stdout)?;
  Ok(output.lines().map(|branch| branch.to_string()).collect())
}

/// Gets the branch's remote tracking branch
fn get_tracking_branch(branch: &str) -> CliResult<String> {
  // --list <pattern> filters the entire list for the given pattern, we can use the current branch
  // name to only find its remote tracking branch
  let output = git!(
    "branch",
    "--format=%(refname:short) %(upstream:short)",
    "--list",
    branch
  )
  .output()?;
  let output = String::from_utf8(output.stdout)?;
  let mut lines = output.lines();

  if let Some(line) = lines.next() {
    let mut words = line.split(" ");
    let _ = words.next();
    let remote = words.next();

    if let Some(it) = remote {
      return Ok(it.to_string());
    }
  }

  Err(CliError::Generic(format!(
    "Couldn't find tracked branch for {}",
    branch
  )))
}

/// Whether branch is merged into base
fn is_merged(branch: &str, base: &str) -> CliResult<bool> {
  let output = git!("log", branch, "--not", base, "--oneline").output()?;
  let output = String::from_utf8(output.stdout)?;
  Ok(output.trim().is_empty())
}

/// Whether there are any uncommitted changes
fn has_local_changes() -> CliResult<bool> {
  // check unstaged changes
  let output = git!("diff")
    .output()
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  let output = String::from_utf8(output.stdout)
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  // if diff is non-empty, immediately return true
  if !output.trim().is_empty() {
    return Ok(true);
  }

  // check staged changes
  let output = git!("diff", "--cached")
    .output()
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  let output = String::from_utf8(output.stdout)
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  Ok(!output.trim().is_empty())
}

/// Whether the branch can be fast-forwarded to its remote counterpart
fn can_fast_forward(branch: &str) -> CliResult<bool> {
  let remote = get_tracking_branch(branch)?;

  let output = git!("rev-parse", branch).output()?;
  let output = String::from_utf8(output.stdout)?;
  let local_sha = output.trim();

  let output = git!("rev-parse", &remote).output()?;
  let output = String::from_utf8(output.stdout)?;
  let remote_sha = output.trim();

  // same sha, branches are up to date
  if local_sha == remote_sha {
    return Ok(true);
  }

  let merge_base = git!("merge-base", "--is-ancestor", branch, remote).status()?;
  Ok(merge_base.success())
}

fn fetch_all() -> CliResult {
  // -p = prune remote refs (e.g. all the origin/<branch>)
  // -t = fetch tags too
  // --all = from all remotes
  await_child!(
    git!("fetch", "-pt", "--all").spawn()?,
    "Failed to fetch from remotes"
  )
}

fn get_term_width() -> usize {
  let (_rows, cols) = console::Term::stdout().size();
  cols as usize
}

/// Configues a prompt and wraps it in a CliResult
fn get_user_confirmation(prompt: &str) -> CliResult<bool> {
  Confirm::new()
    .default(false)
    .with_prompt(prompt)
    .interact()
    .map_err(|_| CliError::Generic("Failed to handle prompt".into()))
}
