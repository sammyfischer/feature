use anyhow::{Context, Result};
use git2::{Branch, Commit, ErrorCode, Oid, Repository, Tag};

use crate::core::string::ToStrLossyOwned;
use crate::core::trim_hash;

pub fn get_current_commit<'repo>(repo: &'repo Repository) -> Result<Option<Commit<'repo>>> {
  let head = match repo.head() {
    Ok(it) => it,
    Err(e) if e.code() == ErrorCode::UnbornBranch => return Ok(None),
    Err(e) => return Err(e.into()),
  };

  let commit = head
    .peel_to_commit()
    .context("Failed to get commit pointed to by HEAD")?;

  Ok(Some(commit))
}

/// Finds a local branch that points to the given commit
pub fn find_branch_at_commit<'repo>(
  repo: &'repo Repository,
  commit_id: &Oid,
) -> Result<Option<Branch<'repo>>> {
  let branches = repo.branches(None)?;

  for (branch, _) in branches.flatten() {
    let id = branch.get().peel_to_commit()?.id();
    if commit_id == &id {
      return Ok(Some(branch));
    }
  }

  Ok(None)
}

/// Finds a tag that points to the given commit
pub fn find_tag_at_commit<'repo>(
  repo: &'repo Repository,
  commit_id: &'repo Oid,
) -> Result<Option<Tag<'repo>>> {
  let tags = repo.tag_names(None)?;

  for name in tags.iter().flatten() {
    let name = name.context("Tag names must be valid utf-8")?;
    let reference = repo.find_reference(&format!("refs/tags/{}", name))?;
    let tag = reference.peel_to_tag()?;
    let tag_commit = reference.peel_to_commit()?;

    if commit_id == &tag_commit.id() {
      return Ok(Some(tag));
    }
  }

  Ok(None)
}

/// Finds a good user-friendly display name for a commit. Tries:
///
/// 1. To find a branch matching the commit, yielding the short branch name
/// 2. To find a tag matching the commit, yielding the short tag name
/// 3. Getting the abbreviated commit hash
pub fn resolve_commit_name(repo: &Repository, commit: &Commit) -> Result<String> {
  if let Some(branch) = find_branch_at_commit(repo, &commit.id())? {
    return Ok(branch.name_bytes()?.to_str_lossy_owned());
  }

  if let Some(tag) = find_tag_at_commit(repo, &commit.id())? {
    return Ok(tag.name_bytes().to_str_lossy_owned());
  }

  trim_hash(commit.as_object())
}
