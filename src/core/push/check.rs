use anyhow::{Result, anyhow};
use git2::{BranchType, Repository};

use crate::core::branch_info::BranchInfo;
use crate::core::fetch::fetch_upstream_branch;
use crate::style;

/// Result of a branch's push check
pub enum PushCheckStatus {
  /// The branch being checked against doesn't exist
  NoBranch,

  /// Ahead/behind checks were not performed, but the branch exists
  Forced,

  /// Both branches point to the same commit
  UpToDate,

  /// Ahead of the branch being checked against
  Ahead,

  /// Behind the branch being checked against
  Behind,

  /// Branches have diverged
  Diverged,
}

/// Fetches the latest upstream ensures that we have all the needed changes
pub fn check_upstream(
  repo: &Repository,
  branch: &BranchInfo,
  upstream: Option<&BranchInfo>,
  force: bool,
) -> Result<PushCheckStatus> {
  let Some(upstream) = upstream else {
    return Ok(PushCheckStatus::NoBranch);
  };

  if !upstream.is_remote() {
    return Err(anyhow!(
      "Upstream is not a remote branch: {}",
      upstream.refname()
    ));
  }

  fetch_upstream_branch(repo, upstream)?;
  println!("{}", style!("Fetched {}", upstream.name()).dim());

  if force {
    return Ok(PushCheckStatus::Forced);
  }

  let branch_tip = branch.resolve(repo)?.peel_to_commit()?;
  let upstream_tip = upstream.resolve(repo)?.peel_to_commit()?;

  // get the new reference after the fetch
  let ab = repo.graph_ahead_behind(branch_tip.id(), upstream_tip.id())?;

  Ok(match ab {
    // up to date, continue to check against base
    (a, b) if a == 0 && b == 0 => PushCheckStatus::UpToDate,

    // local is ahead, continue with push (and check against base)
    (a, b) if a > 0 && b == 0 => PushCheckStatus::Ahead,

    // local is behind, fast forward (soft reset)
    (a, b) if a == 0 && b > 0 => PushCheckStatus::Behind,

    // divergent histories, user must resolve
    (a, b) if a > 0 && b > 0 => PushCheckStatus::Diverged,

    (a, b) => {
      return Err(anyhow!(
        "Unexpected ahead/behind against upstream: ahead {}, behind {}",
        a,
        b
      ));
    }
  })
}

/// Fetches the latest base ensures that we have all the needed changes
pub fn check_base(
  repo: &Repository,
  branch: &BranchInfo,
  base: Option<&BranchInfo>,
  force: bool,
) -> Result<PushCheckStatus> {
  let Some(base) = base else {
    return Ok(PushCheckStatus::NoBranch);
  };

  if base.ty() == BranchType::Remote {
    fetch_upstream_branch(repo, base)?;
    println!("{}", style!("Fetched {}", base.name()).dim());
  }

  if force {
    return Ok(PushCheckStatus::Forced);
  }

  let local = branch.resolve(repo)?.peel_to_commit()?.id();
  let upstream = base.resolve(repo)?.peel_to_commit()?.id();
  let ab = repo.graph_ahead_behind(local, upstream)?;

  Ok(match ab {
    // already up to date, continue with push
    (a, b) if a == 0 && b == 0 => PushCheckStatus::UpToDate,

    // branch is ahead, continue with push
    (a, b) if a > 0 && b == 0 => PushCheckStatus::Ahead,

    // branch is behind, need those changes
    (a, b) if a == 0 && b > 0 => PushCheckStatus::Behind,

    // divergent histories, user must resolve
    (a, b) if a > 0 && b > 0 => PushCheckStatus::Diverged,

    (a, b) => {
      return Err(anyhow!(
        "Unexpected ahead/behind against upstream: ahead {}, behind {}",
        a,
        b
      ));
    }
  })
}
