//! Helper functions pertaining to branches

use std::borrow::Cow;

use anyhow::{Context, Result, anyhow};
use git2::{AutotagOption, Branch, BranchType, ErrorCode, FetchOptions, FetchPrune, Repository};

use crate::lossy;
use crate::util::get_remote_callbacks;

pub fn branch_to_name<'repo>(branch: &'repo Branch) -> Result<Cow<'repo, str>> {
  Ok(lossy!(&branch.name_bytes()?))
}

pub fn name_to_branch<'repo>(repo: &'repo Repository, name: &str) -> Result<Branch<'repo>> {
  let branch = repo
    .find_branch(name, BranchType::Local)
    .with_context(|| format!("Failed to find branch named {}", name))?;
  Ok(branch)
}

pub fn name_to_remote_branch<'repo>(repo: &'repo Repository, name: &str) -> Result<Branch<'repo>> {
  let branch = repo
    .find_branch(name, BranchType::Remote)
    .with_context(|| format!("Failed to find branch named {}", name))?;
  Ok(branch)
}

pub fn get_upstream<'repo>(branch: &Branch<'repo>) -> Result<Option<Branch<'repo>>> {
  match branch.upstream() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Unknown error when trying to get upstream")),
  }
}

pub fn get_current_branch_name(repo: &Repository) -> Result<String> {
  let head = repo.head()?;

  if !head.is_branch() {
    return Err(anyhow!("Not checked out to a branch"));
  }

  Ok(lossy!(head.shorthand_bytes()).to_string())
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

pub fn get_ahead_behind<'repo>(
  repo: &'repo Repository,
  branch: &Branch<'repo>,
  upstream: &Branch<'repo>,
) -> Result<(usize, usize)> {
  let branch_tip = branch
    .get()
    .peel_to_commit()
    .context("Failed to get branch commit when getting ahead/behind")?
    .id();

  let upstream_tip = upstream
    .get()
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
