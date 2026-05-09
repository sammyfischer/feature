//! Helper functions pertaining to branches

use std::borrow::Cow;

use anyhow::{Context, Result, anyhow};
use git2::{
  AutotagOption, Branch, BranchType, Commit, ErrorCode, FetchOptions, FetchPrune, ObjectType, Oid,
  Reference, RemoteCallbacks, Repository, ResetType,
};

use crate::util::branch_meta::BranchMeta;
use crate::util::display::trim_hash;
use crate::util::lossy::{ToStrLossy, ToStrLossyOwned};
use crate::util::{credentials_cb, get_current_commit};

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

pub fn branch_to_name<'repo>(branch: &'repo Branch) -> Result<Cow<'repo, str>> {
  Ok(branch.name_bytes()?.to_str_lossy())
}

pub fn branch_to_commit<'repo>(branch: &Branch<'repo>) -> Result<Option<Commit<'repo>>> {
  match branch.get().peel_to_commit() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context(format!(
      "Failed to get commit at branch {}",
      branch_to_name(branch).unwrap_or(Cow::Borrowed("<unknown>"))
    ))),
  }
}

/// Iterates through all local (refs/heads/*) and remote (refs/remotes/*) branches to find one that
/// points to the given commit
pub fn commit_to_branch<'repo>(
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

/// Get the name of the current branch, or the trimmed hash if the repo is in detached HEAD, or None
/// if the repo is empty
pub fn get_current_branch_or_commit(repo: &Repository) -> Result<Option<String>> {
  Ok(match get_current_branch_name(repo) {
    Err(e) => return Err(e),

    Ok(branch) => match branch {
      Some(branch) => Some(branch),

      // no current branch, get commit instead
      None => match get_current_commit(repo) {
        Err(e) => return Err(e),

        Ok(commit) => match commit {
          Some(commit) => Some(trim_hash(&commit)?),
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
  upstream: &BranchMeta,
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

pub fn get_worktree_branch_names(repo: &Repository) -> Result<Vec<String>> {
  let mut names = Vec::new();

  for name in repo.worktrees()?.iter().flatten() {
    let wt = repo.find_worktree(name)?;
    let wt_repo = Repository::open_from_worktree(&wt)?;
    let branch = get_current_branch_name(&wt_repo)?;
    if let Some(branch) = branch {
      names.push(branch);
    }
  }

  Ok(names)
}

pub fn get_ahead_behind<'repo>(
  repo: &'repo Repository,
  branch: &Reference<'repo>,
  upstream: &Reference<'repo>,
) -> Result<(usize, usize)> {
  let branch_tip = branch
    .peel_to_commit()
    .context("Failed to get branch commit when getting ahead/behind")?
    .id();

  let upstream_tip = upstream
    .peel_to_commit()
    .context("Failed to get upstream commit when getting ahead/behind")?
    .id();

  let ab = repo
    .graph_ahead_behind(branch_tip, upstream_tip)
    .context("Failed to get ahead/behind")?;
  Ok(ab)
}

/// Fetches all remote branches
pub fn fetch_all(repo: &Repository) -> Result<()> {
  let remotes = repo.remotes().expect("Failed to list all remotes");
  let mut results: Vec<Result<()>> = Vec::with_capacity(remotes.len());

  for remote_name in &remotes {
    let Some(remote_name) = remote_name else {
      continue;
    };

    let mut remote = repo
      .find_remote(remote_name)
      .unwrap_or_else(|_| panic!("Failed to get reference to remote {}", remote_name));
    let mut cbs = RemoteCallbacks::new();
    cbs.credentials(credentials_cb);

    let mut opts = FetchOptions::new();
    opts.remote_callbacks(cbs);
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

/// Fetch a single branch. `branch` must be a remote branch.
pub fn fetch_upstream_branch(repo: &Repository, branch: &BranchMeta) -> Result<()> {
  assert!(
    branch.ty() == BranchType::Remote,
    "Cannot fetch {}: not a remote branch",
    branch.refname()
  );

  let (shortname, remote_name) = branch.split_name_and_remote()?;
  let mut remote = repo.find_remote(
    &remote_name
      .unwrap_or_else(|| panic!("Remote should exist on upstream branch: {}", branch.name())),
  )?;

  let refspec = format!("+refs/heads/{}:{}", shortname, branch.refname());

  let mut opts = FetchOptions::new();
  let mut cbs = RemoteCallbacks::new();
  cbs.credentials(credentials_cb);
  opts.remote_callbacks(cbs);

  remote.fetch(&[&refspec], Some(&mut opts), None)?;
  Ok(())
}

/// Reset current branch and HEAD to `branch`
pub fn hard_reset(repo: &Repository, branch: &Reference) -> Result<()> {
  let obj = repo.find_object(branch.peel_to_commit()?.id(), Some(ObjectType::Commit))?;
  repo.reset(&obj, ResetType::Hard, None)?;
  Ok(())
}

/// Switch to the given branch (checks out the branch and updates HEAD)
pub fn switch(repo: &Repository, branch: &BranchMeta) -> Result<()> {
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

  // whether branch is a descendant of base. if it is, then there are newer unmerged commits
  let is_descendant = repo.graph_descendant_of(branch_commit, base_commit)?;
  Ok(!is_descendant)
}
