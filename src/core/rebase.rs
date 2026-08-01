use std::fs;

use anyhow::Result;
use git2::{Oid, Repository, RepositoryState};

/// Info about the rebase
pub struct RebaseInfo {
  /// Current rebase step (1-indexed)
  current: usize,

  /// Total number of rebase steps
  total: usize,

  /// The name of the source of the rebase commits. Should use
  /// [Repository::resolve_reference_from_short_name] to get the actual
  /// reference.
  head: String,

  /// The commit id of the rebase destination
  onto: Oid,
}

impl RebaseInfo {
  pub fn get(repo: &Repository) -> Result<Option<Self>> {
    let dir = repo.path().join(match repo.state() {
      RepositoryState::Rebase
      | RepositoryState::RebaseInteractive
      | RepositoryState::RebaseMerge => "rebase-merge",
      RepositoryState::ApplyMailboxOrRebase => "rebase-apply",
      _ => return Ok(None),
    });

    let current = {
      let path = dir.join("msgnum");
      let text = fs::read_to_string(path)?;
      text.trim().parse::<usize>()?
    };

    let total = {
      let path = dir.join("end");
      let text = fs::read_to_string(path)?;
      text.trim().parse::<usize>()?
    };

    let head = {
      let path = dir.join("head-name");
      fs::read_to_string(path)?
    };

    let onto = {
      let path = dir.join("onto");
      let text = fs::read_to_string(path)?;
      Oid::from_str(text.trim())?
    };

    Ok(Some(Self {
      current,
      total,
      head,
      onto,
    }))
  }

  pub fn current(&self) -> usize {
    self.current
  }

  pub fn total(&self) -> usize {
    self.total
  }

  pub fn head(&self) -> &str {
    &self.head
  }

  pub fn onto(&self) -> Oid {
    self.onto
  }
}
