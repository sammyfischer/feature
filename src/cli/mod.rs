//! Defines the main cli structure, most simple commands, and several helper functions and macros.

use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use git2::{
  AutotagOption,
  BranchType,
  Cred,
  CredentialType,
  FetchOptions,
  FetchPrune,
  RemoteCallbacks,
  Repository,
  Status,
  StatusOptions,
};

use crate::cli::error::CliError;
use crate::cli_err;
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
      Err($crate::cli::error::CliError::Process($msg.to_string()))
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
fn get_current_branch(repo: &Repository) -> CliResult<String> {
  let head = repo.head()?;

  if !head.is_branch() {
    return Err(cli_err!(Git, "Not checked out to a branch"));
  }

  let short = head.shorthand().ok_or(cli_err!(
    Git,
    "HEAD has no shorthand. Are you checked out to a branch?"
  ))?;

  Ok(short.to_string())
}

/// Returns a list of all local branches, or None if there was an error getting output
fn get_all_branches(repo: &Repository) -> CliResult<Vec<String>> {
  let branches = repo.branches(Some(BranchType::Local))?;

  let mut output: Vec<String> = Vec::new();

  // unwrap results and options, skip on error or none
  for b in branches {
    let Ok((b, _)) = b else {
      continue;
    };
    let Ok(Some(name)) = b.name() else {
      continue;
    };
    output.push(name.to_string());
  }

  Ok(output)
}

/// Gets the branch's remote tracking branch
fn get_tracking_branch(repo: &Repository, branch: &str) -> CliResult<String> {
  let upstream = repo.branch_upstream_remote(&format!("refs/heads/{}", branch))?;

  let name = upstream
    .as_str()
    .ok_or(cli_err!(Git, "Failed to get upstream of {}", branch))?;

  Ok(name.to_string())
}

/// Whether branch is merged into base
fn is_merged(branch: &str, base: &str) -> CliResult<bool> {
  let output = git!("log", branch, "--not", base, "--oneline").output()?;
  let output = String::from_utf8(output.stdout)?;
  Ok(output.trim().is_empty())
}

/// Whether there are any uncommitted changes
fn has_local_changes(repo: &Repository) -> CliResult<bool> {
  let mut opts = StatusOptions::new();
  opts.include_untracked(false);
  let statuses = repo.statuses(Some(&mut opts))?;

  for entry in &statuses {
    let status = entry.status();

    if status.intersects(
      Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE
        | Status::WT_MODIFIED
        | Status::WT_DELETED
        | Status::WT_RENAMED
        | Status::WT_TYPECHANGE,
    ) {
      // return true immediately if any of the above changes are found
      return Ok(true);
    }
  }

  Ok(false)
}

/// Whether the branch can be fast-forwarded to its remote counterpart
fn can_fast_forward(repo: &Repository, branch: &str) -> CliResult<bool> {
  let upstream = get_tracking_branch(repo, branch)?;

  let local_obj = repo.find_branch(branch, BranchType::Local)?;
  let upstream_obj = repo.find_branch(&upstream, BranchType::Remote)?;

  let local_oid = local_obj.get().peel_to_commit()?.id();
  let upstream_oid = upstream_obj.get().peel_to_commit()?.id();

  // same sha, branches are up to date
  if local_oid == upstream_oid {
    return Ok(true);
  }

  Ok(repo.merge_base(local_oid, upstream_oid).is_ok())
}

fn fetch_all(repo: &Repository) -> CliResult {
  let mut results: Vec<CliResult> = Vec::new();

  let remotes = repo.remotes()?;
  for remote_name in &remotes {
    let Some(remote_name) = remote_name else {
      continue;
    };

    let mut remote = repo.find_remote(remote_name)?;
    let callbacks = get_remote_callbacks();

    let mut opts = FetchOptions::new();
    opts.remote_callbacks(callbacks);
    opts.prune(FetchPrune::On);
    opts.download_tags(AutotagOption::All);

    results.push(
      remote
        .fetch(
          &[format!("refs/heads/*:refs/remotes/{}/*", remote_name)],
          Some(&mut opts),
          None,
        )
        .map_err(|e| e.into()),
    );
  }

  results.into_iter().collect()
}

/// Gets remote callbacks to use for remote operations with git2
fn get_remote_callbacks<'repo>() -> RemoteCallbacks<'repo> {
  let mut callbacks = RemoteCallbacks::new();

  callbacks.credentials(|url, username_from_url, allowed_types| {
    if allowed_types.contains(CredentialType::SSH_KEY) {
      return Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
    }

    if allowed_types.contains(CredentialType::USER_PASS_PLAINTEXT) {
      if let Ok(cred) =
        Cred::credential_helper(&git2::Config::open_default()?, url, username_from_url)
      {
        return Ok(cred);
      }

      // fallback to git token env var
      let token = std::env::var("GIT_TOKEN").map_err(|_| {
        git2::Error::from_str(
          "Failed to find credentials. Try setting the GIT_TOKEN environment variable",
        )
      })?;

      return Cred::userpass_plaintext(username_from_url.unwrap_or("git"), &token);
    }

    if allowed_types.contains(CredentialType::DEFAULT) {
      return Cred::default();
    }

    Err(git2::Error::from_str(&format!(
      "No supported credential type for {url}"
    )))
  });

  callbacks
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
