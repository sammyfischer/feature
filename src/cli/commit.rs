//! Commit subcommand

use std::process::Command;

use anyhow::{Result, anyhow};
use git2::{Commit, Repository};

use crate::cli::get_current_commit;
use crate::open_repo;

const AMEND_LONG_HELP: &str = r"Amend the previous commit. Remaining args overwrite the previous commit message.
If no remaining args are specified, the previous commit message is used.";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Commit staged changes")]
pub struct Args {
  /// Whether to amend the previous commit
  #[arg(long, long_help = AMEND_LONG_HELP)]
  amend: bool,

  /// Bypass precommit hooks
  #[arg(long)]
  no_verify: bool,

  /// Words to join together as commit message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  words: Vec<String>,
}

impl Args {
  pub fn run(&self) -> Result<()> {
    let repo = open_repo!();
    let msg = self.words.join(" ");

    // most recent commit, i.e. commit that HEAD points to. None when repository has no commits
    let current_commit = get_current_commit(&repo)?;

    // all the info needed for amend
    if self.amend {
      let current_commit = current_commit.ok_or(anyhow!("No commits yet, cannot amend"))?;
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
        .expect("Failed to amend commit");

      return Ok(());
    }

    // not an amend, must specify a message
    if msg.is_empty() {
      return Err(anyhow!("Must specify a commit message"));
    }

    // extra info to create a commit
    let signature = repo
      .signature()
      .expect("Failed to get default commit signature");

    let mut index = repo.index().expect("Failed to get current index");
    let tree_id = index.write_tree().expect("Failed to get index tree id");
    let tree = repo.find_tree(tree_id).expect("Failed to get index tree");

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
      .expect("Failed to commit");

    Ok(())
  }

  fn pre_commit(&self, repo: &Repository) -> Result<()> {
    if self.no_verify {
      return Ok(());
    }

    let git_dir = repo.path();
    let script = git_dir.join("hooks").join("pre-commit");

    if !script.exists() {
      // no hooks set, always succeed
      return Ok(());
    }

    let status = Command::new(script).status()?;

    if status.success() {
      Ok(())
    } else {
      Err(anyhow!("Precommit hooks failed"))
    }
  }
}
