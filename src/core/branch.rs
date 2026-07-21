//! Helper functions for branches and references

use anyhow::{Context, Result, anyhow};
use git2::{
  Branch,
  BranchType,
  ErrorClass,
  ErrorCode,
  ObjectType,
  Reference,
  Repository,
  ResetType,
};

use crate::core::branch_info::BranchInfo;
use crate::core::commit::get_current_commit;
use crate::core::{NotFoundExt, trim_hash};

/// Gets HEAD and resolves to a direct reference
///
/// # Returns
/// The direct reference to the commit pointed to by HEAD, or `None` if the
/// branch is unborn (i.e. there are no commits yet)
pub fn get_head_resolved<'rf>(repo: &'rf Repository) -> Result<Option<Reference<'rf>>> {
  match repo.head() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.class() == ErrorClass::Reference && e.code() == ErrorCode::UnbornBranch => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get reference to HEAD")),
  }
}

/// Gets HEAD without resolving. Most of the time, you should use
/// [get_head_resolved].
///
/// [get_head_resolved]: crate::core::branch::get_head_resolved
pub fn get_head<'rf>(repo: &'rf Repository) -> Result<Option<Reference<'rf>>> {
  repo.find_reference("HEAD").not_found_ok()
}

pub fn get_merge_head<'rf>(repo: &'rf Repository) -> Result<Option<Reference<'rf>>> {
  repo.find_reference("MERGE_HEAD").not_found_ok()
}

pub fn get_pick_head<'rf>(repo: &'rf Repository) -> Result<Option<Reference<'rf>>> {
  repo.find_reference("CHERRY_PICK_HEAD").not_found_ok()
}

pub fn get_revert_head<'rf>(repo: &'rf Repository) -> Result<Option<Reference<'rf>>> {
  repo.find_reference("REVERT_HEAD").not_found_ok()
}

/// Get the name of the current branch, or the trimmed hash if the repo is in
/// detached HEAD, or None if the repo is empty
pub fn get_current_branch_or_commit(repo: &Repository) -> Result<Option<String>> {
  if let Some(name) = get_current_branch_name(repo)? {
    return Ok(Some(name));
  };

  if let Some(commit) = get_current_commit(repo)? {
    return Ok(Some(trim_hash(commit.as_object())?));
  }

  Ok(None)
}

/// Gets the name of the currently checked-out branch.
///
/// # Returns
/// `None` if:
/// - HEAD doesn't exist (unlikely)
/// - HEAD doesn't point to a branch
/// - HEAD points to an unborn branch (there are no commits in repo)
pub fn get_current_branch_name(repo: &Repository) -> Result<Option<String>> {
  match get_head_resolved(repo)? {
    Some(rf) => {
      if !rf.is_branch() {
        return Ok(None);
      }

      Ok(Some(rf.shorthand()?.to_string()))
    }
    None => Ok(None),
  }
}

/// Finds the local copy of an upstream tracking branch
pub fn find_local_of_upstream<'branch>(
  repo: &'branch Repository,
  upstream: &BranchInfo,
) -> Result<Option<Branch<'branch>>> {
  for (branch, _) in repo.branches(Some(BranchType::Local))?.flatten() {
    let Some(branch_upstream) = branch.upstream().not_found_ok()? else {
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
