//! Commit subcommand

use std::process::Command;

use git2::{Commit, ErrorCode, Repository};

use crate::cli::error::CliError;
use crate::cli::{CliResult, get_current_commit};
use crate::{cli_err, cli_err_fn};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// Whether to amend the previous commit
  #[arg(
    long,
    long_help = "Amend the previous commit. Remaining args overwrite the previous commit message. If no remaining args are specified, the previous commit message is preserved."
  )]
  amend: bool,

  /// Bypass precommit hooks
  #[arg(long)]
  no_verify: bool,

  /// Words to join together as commit message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  words: Vec<String>,
}

impl Args {
  pub fn run(&self) -> CliResult {
    let repo = Repository::open_from_env()?;
    let msg = self.words.join(" ");

    // most recent commit, i.e. commit that HEAD points to. None when repository has no commits
    let current_commit = match get_current_commit(&repo) {
      Ok(it) => Some(it),
      // empty repository, HEAD points to nothing
      Err(e) if e.code() == ErrorCode::UnbornBranch => None,
      Err(e) => return Err(cli_err!(Git, "Failed to get HEAD: {e}")),
    };

    // all the info needed for amend
    if self.amend {
      let current_commit = current_commit.ok_or(cli_err!(Git, "No commits yet, cannot amend"))?;

      self.pre_commit(&repo)?;

      current_commit
        .amend(
          Some("HEAD"),
          None,
          None,
          None,
          if !msg.is_empty() { Some(&msg) } else { None },
          None,
        )
        .map_err(cli_err_fn!(Git, e, "Failed to amend commit: {e}"))?;

      return Ok(());
    }

    // not an amend, must specify a message
    if msg.is_empty() {
      return Err(CliError::Generic("Must specify a commit message".into()));
    }

    // extra info to create a commit
    let signature =
      repo
        .signature()
        .map_err(cli_err_fn!(Git, e, "Failed to get default signature: {e}"))?;

    let mut index = repo
      .index()
      .map_err(cli_err_fn!(Git, e, "Failed to get index: {e}"))?;

    let tree_id =
      index
        .write_tree()
        .map_err(cli_err_fn!(Git, e, "Failed to get index tree id: {e}"))?;

    let tree = repo.find_tree(tree_id).map_err(cli_err_fn!(
      Git,
      e,
      "Failed find index tree from id: {e}"
    ))?;

    let parent_commits: Vec<Commit> = current_commit.into_iter().collect();
    let parent_refs: Vec<&Commit> = parent_commits.iter().collect();

    self.pre_commit(&repo)?;

    repo
      .commit(
        Some("HEAD"),
        &signature,
        &signature,
        &msg,
        &tree,
        &parent_refs,
      )
      .map_err(cli_err_fn!(Git, e, "Failed to commit: {e}"))?;

    Ok(())
  }

  fn pre_commit(&self, repo: &Repository) -> CliResult {
    if self.no_verify {
      return Ok(());
    }

    let git_dir = repo.path();
    let script = git_dir.join("hooks").join("pre-commit");

    if !script.exists() {
      // no hooks set, always succeed
      return Ok(());
    }

    match Command::new(script).status() {
      Ok(it) if it.success() => Ok(()),
      Ok(_) => Err(cli_err!(Precommit, "Precommit hooks failed")),
      Err(e) => Err(cli_err!(Process, "Failed to run precommit hooks: {e}")),
    }
  }
}
