//! Helper functions pertaining to branches

use std::borrow::Cow;

use anyhow::{Context, Result, anyhow};
use git2::{
  AutotagOption,
  Branch,
  BranchType,
  Commit,
  ErrorCode,
  FetchOptions,
  FetchPrune,
  Oid,
  Reference,
  Repository,
};

use crate::lossy;
use crate::util::display::trim_hash;
use crate::util::{get_current_commit, get_remote_callbacks};

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
  Ok(lossy!(&branch.name_bytes()?))
}

/// Searches local and remote branches to find one matching the given name. Returns None when no
/// matching branch is found.
pub fn name_to_branch<'repo>(repo: &'repo Repository, name: &str) -> Result<Option<Branch<'repo>>> {
  match repo.find_branch(name, BranchType::Local) {
    Ok(branch) => Ok(Some(branch)),

    Err(e) if e.code() == ErrorCode::NotFound => match repo.find_branch(name, BranchType::Remote) {
      Ok(branch) => Ok(Some(branch)),

      Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
      Err(e) => Err(anyhow!(e)),
    },
    Err(e) => Err(anyhow!(e)),
  }
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

        Ok(commit) => commit.map(|commit| trim_hash(&commit.id())),
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

      Ok(Some(lossy!(head.shorthand_bytes()).to_string()))
    }
    None => Ok(None),
  }
}

pub fn get_all_branch_names(repo: &Repository) -> Result<Vec<String>> {
  let branches = repo.branches(Some(BranchType::Local))?;
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
