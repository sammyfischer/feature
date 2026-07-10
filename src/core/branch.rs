//! Helper functions for branches and references

use anyhow::{Context, Result, anyhow};
use git2::{Branch, BranchType, ErrorCode, ObjectType, Reference, Repository, ResetType};

use crate::core::branch_info::BranchInfo;
use crate::core::commit::get_current_commit;
use crate::core::string::ToStrLossyOwned;
use crate::core::trim_hash;

pub fn get_head<'repo>(repo: &'repo Repository) -> Result<Option<Reference<'repo>>> {
  match repo.head() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::UnbornBranch => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get reference to HEAD")),
  }
}

pub fn get_merge_head<'repo>(repo: &'repo Repository) -> Result<Option<Reference<'repo>>> {
  match repo.find_reference("MERGE_HEAD") {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get reference to MERGE_HEAD")),
  }
}

pub fn get_pick_head<'repo>(repo: &'repo Repository) -> Result<Option<Reference<'repo>>> {
  match repo.find_reference("CHERRY_PICK_HEAD") {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get reference to CHERRY_PICK_HEAD")),
  }
}

pub fn get_revert_head<'repo>(repo: &'repo Repository) -> Result<Option<Reference<'repo>>> {
  match repo.find_reference("REVERT_HEAD") {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get reference to REVERT_HEAD")),
  }
}

/// Get the name of the current branch, or the trimmed hash if the repo is in
/// detached HEAD, or None if the repo is empty
pub fn get_current_branch_or_commit(repo: &Repository) -> Result<Option<String>> {
  Ok(match get_current_branch_name(repo) {
    Err(e) => return Err(e),

    Ok(branch) => match branch {
      Some(branch) => Some(branch),

      // no current branch, get commit instead
      None => match get_current_commit(repo) {
        Err(e) => return Err(e),

        Ok(commit) => match commit {
          Some(commit) => Some(trim_hash(commit.as_object())?),
          None => None,
        },
      },
    },
  })
}

pub fn get_upstream<'repo>(branch: &Branch<'repo>) -> Result<Option<Branch<'repo>>> {
  match branch.upstream() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Unknown error when trying to get upstream")),
  }
}

pub fn get_current_branch_name(repo: &Repository) -> Result<Option<String>> {
  match get_head(repo)? {
    Some(head) => {
      if !head.is_branch() {
        return Ok(None);
      }

      Ok(Some(head.shorthand_bytes().to_str_lossy_owned()))
    }
    None => Ok(None),
  }
}

/// Finds the local copy of an upstream tracking branch
pub fn find_local_of_upstream<'repo>(
  repo: &'repo Repository,
  upstream: &BranchInfo,
) -> Result<Option<Branch<'repo>>> {
  for (branch, _) in repo.branches(Some(BranchType::Local))?.flatten() {
    let Some(branch_upstream) = get_upstream(&branch)? else {
      continue;
    };
    if upstream.refname().as_bytes() == branch_upstream.get().name_bytes() {
      return Ok(Some(branch));
    }
  }

  Ok(None)
}

/// Gets the names of branches that worktrees are checked-out to
pub fn get_worktree_branch_names(repo: &Repository) -> Result<Vec<String>> {
  let mut names = Vec::new();

  for name in repo.worktrees()?.iter().flatten() {
    let name = name.context("Worktree names must be valid utf-8")?;
    let wt = repo.find_worktree(name)?;
    let wt_repo = Repository::open_from_worktree(&wt)?;
    let branch = get_current_branch_name(&wt_repo)?;
    if let Some(branch) = branch {
      names.push(branch);
    }
  }

  Ok(names)
}

pub fn get_ahead_behind(
  repo: &Repository,
  branch: &Reference,
  upstream: &Reference,
) -> Result<(usize, usize)> {
  let branch_tip = branch.peel_to_commit()?.id();
  let upstream_tip = upstream.peel_to_commit()?.id();
  let ab = repo.graph_ahead_behind(branch_tip, upstream_tip)?;
  Ok(ab)
}

/// Reset current branch and HEAD to `branch`
pub fn hard_reset(repo: &Repository, branch: &Reference) -> Result<()> {
  let obj = repo.find_object(branch.peel_to_commit()?.id(), Some(ObjectType::Commit))?;
  repo.reset(&obj, ResetType::Hard, None)?;
  Ok(())
}

/// Switch to the given branch (checks out the branch and updates HEAD)
pub fn switch(repo: &Repository, branch: &BranchInfo) -> Result<()> {
  let reference = branch.resolve(repo)?;
  let tree = reference.peel_to_tree()?;
  let obj = tree.as_object();
  repo.checkout_tree(obj, None)?;
  repo.set_head(branch.refname())?;
  Ok(())
}

/// Whether branch is merged into base. A branch is considered merged if:
/// - it points to the same commit as its base
/// - it's not a descendant of base (i.e. there are no new commits)
pub fn is_merged(repo: &Repository, branch: &Reference, base: &Reference) -> Result<bool> {
  let branch_commit = branch.peel_to_commit()?.id();
  let base_commit = base.peel_to_commit()?.id();

  if branch_commit == base_commit {
    return Ok(true);
  }

  // whether branch is a descendant of base. if it is, then there are newer
  // unmerged commits
  let is_descendant = repo.graph_descendant_of(branch_commit, base_commit)?;
  Ok(!is_descendant)
}
