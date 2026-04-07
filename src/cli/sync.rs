use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{BranchType, FetchOptions, ObjectType, Repository, Status, StatusOptions};

use crate::cli::{Cli, fetch_all, get_current_branch, get_remote_callbacks};
use crate::{lossy, open_repo};

pub const LONG_ABOUT: &str = r"Updates all base branches with their remotes.

Fast-forwards local branches (e.g. refs/heads/*) and force-updates remotes
(e.g. refs/remotes/origin/*).";

pub fn run(cli: &Cli) -> Result<()> {
  let repo = open_repo!();
  fetch_all(&repo)?;

  let current_branch = get_current_branch(&repo).context("Failed to get current branch")?;
  let bases = &cli.config.bases;

  let mut opts = FetchOptions::new();
  opts.remote_callbacks(get_remote_callbacks());

  for branch_name in bases {
    let is_current = branch_name == &current_branch;
    if is_current {
      // check for local changes
      if has_local_changes(&repo)? {
        eprintln!(
          r"Cannot update {} with uncommitted changes. You resolve this by:

1. Stashing changes
  git stash push <message>
  feature sync or git pull

2. Resetting and discarding the changes
  git reset --hard <upstream_name>
",
          branch_name
        );
        continue;
      }
    }

    if let Err(e) = fast_forward(&repo, branch_name, is_current) {
      eprintln!("Failed to update: {}", e);
      continue;
    }

    println!("{} {}", style("Updated").green(), branch_name);
  }

  Ok(())
}

/// Merges a branch with its upstream if it can be fast-forwarded. Set `current` to true when
/// fast-forwarding the currently checked-out branch.
fn fast_forward(repo: &Repository, branch_name: &str, current: bool) -> Result<()> {
  let branch = repo.find_branch(branch_name, BranchType::Local)?;
  let upstream = branch.upstream()?;
  let upstream_name = lossy!(upstream.name_bytes()?);

  let branch_tip = branch.get().peel_to_commit()?;
  let upstream_tip = upstream.get().peel_to_commit()?;

  // already up to date
  if branch_tip.id() == upstream_tip.id() {
    return Ok(());
  }

  let can_ff = repo.graph_descendant_of(upstream_tip.id(), branch_tip.id())?;

  if !can_ff {
    return Err(anyhow!(
      r"{0} cannot be fast-forwarded. You can resolve this by:

1. Forcing the branches to match:
  git checkout {0}
  git reset --hard {1}

2. Manually merging or rebasing:
  git checkout {0}
  git merge/rebase {1}

Option (1) is recommended for base branches that are supposed to reflect the
remote copy rather than be modified directly (e.g. if you're working on a
project with others, or the branch has branch protections on the remote).",
      branch_name,
      upstream_name
    ));
  }

  if current {
    // to update the current branch, we also need to update HEAD. this is just a hard reset
    let obj = repo.find_object(upstream_tip.id(), Some(ObjectType::Commit))?;
    repo.reset(&obj, git2::ResetType::Hard, None)?;
  } else {
    // for other branches, we just move them to the upstream commit
    branch
      .into_reference()
      .set_target(upstream_tip.id(), "feature sync fast-forward")?;
  }

  Ok(())
}

/// Whether there are any uncommitted changes
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
