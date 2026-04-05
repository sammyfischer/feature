//! Defines the main cli structure, most simple commands, and several helper functions and macros.

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use git2::{
  AutotagOption,
  BranchType,
  Commit,
  Cred,
  CredentialType,
  ErrorCode,
  FetchOptions,
  FetchPrune,
  RemoteCallbacks,
  Repository,
  Status,
  StatusOptions,
};

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
  Update(update::Args),
  Push(push::Args),

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  #[command(about = "Syncs all base branches with their remotes", long_about = sync::LONG_ABOUT)]
  Sync,

  Prune(prune::Args),

  // ==== DISPLAY / INFO ====
  #[command(visible_alias = "ls")]
  List(list::Args),

  Log(log::Args),
  Graph(graph::Args),

  // ==== META / FEATURE COMMANDS ====
  /// Interact with feature config
  Config {
    #[command(subcommand)]
    args: config_cmd::Args,
  },

  Base(base::Args),
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
      Action::Update(args) => args.run(),
      Action::Push(args) => args.run(self),
      Action::Sync => sync::run(self),
      Action::Prune(args) => args.run(self),
      Action::List(args) => args.run(),
      Action::Log(args) => args.run(self),
      Action::Graph(args) => args.run(self),
      Action::Config { args } => args.run(),
      Action::Base(args) => args.run(),
    }
  }
}

fn get_current_commit<'repo>(repo: &'repo Repository) -> Result<Option<Commit<'repo>>> {
  let head = match repo.head() {
    Ok(it) => it,
    Err(e) if e.code() == ErrorCode::UnbornBranch => return Ok(None),
    Err(e) => return Err(e.into()),
  };

  let commit = head
    .peel_to_commit()
    .expect("Failed to get commit pointed to by HEAD");

  Ok(Some(commit))
}

/// Gets current branch name
fn get_current_branch(repo: &Repository) -> Result<String> {
  let head = repo.head()?;

  if !head.is_branch() {
    return Err(anyhow!("Not checked out to a branch"));
  }

  let short = head.shorthand().ok_or(anyhow!(
    "HEAD has no shorthand. Are you checked out to a branch?"
  ))?;

  Ok(short.to_string())
}

/// Returns a list of all local branches
fn get_all_branches(repo: &Repository) -> Result<Vec<String>> {
  let branches = repo
    .branches(Some(BranchType::Local))
    .expect("Failed to get list of local branches");

  let mut output: Vec<String> = Vec::new();

  // unwrap results and options, skip on error or none
  for branch in branches {
    if let Ok((branch, _)) = branch
      && let Ok(Some(name)) = branch.name()
    {
      output.push(name.to_string());
    }
  }

  Ok(output)
}

/// Whether there are any uncommitted changes
#[allow(unused)]
fn has_local_changes(repo: &Repository) -> Result<bool> {
  let mut opts = StatusOptions::new();
  opts.include_untracked(false);

  let statuses = repo
    .statuses(Some(&mut opts))
    .expect("Failed to get repository statuses");

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

/// Fetches all remote branches
fn fetch_all(repo: &Repository) -> Result<()> {
  let mut results: Vec<Result<()>> = Vec::new();

  let remotes = repo.remotes().expect("Failed to list all remotes");
  for remote_name in &remotes {
    let Some(remote_name) = remote_name else {
      continue;
    };

    let mut remote = repo
      .find_remote(remote_name)
      .unwrap_or_else(|_| panic!("Failed to get reference to remote {}", remote_name));
    let callbacks = get_remote_callbacks();

    let mut opts = FetchOptions::new();
    opts.remote_callbacks(callbacks);
    opts.prune(FetchPrune::On);
    opts.download_tags(AutotagOption::All);

    results.push(
      remote
        .fetch(
          &[format!("+refs/heads/*:refs/remotes/{}/*", remote_name)],
          Some(&mut opts),
          None,
        )
        .map_err(|e| anyhow!("{}", e)),
    );
  }

  for result in results {
    if let Err(e) = result {
      eprintln!("{}", e);
    }
  }

  Ok(())
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
  let (_rows, cols) = console::Term::stdout().size_checked().unwrap_or((64, 80));
  cols as usize
}

/// Configues a yes/no prompt and gets user input
fn get_user_confirmation(prompt: &str) -> Result<bool> {
  let result = Confirm::new()
    .default(false)
    .with_prompt(prompt)
    .interact()?;
  Ok(result)
}
