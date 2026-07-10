use std::fmt::Display;

use anyhow::{Context, Result};
use console::style;
use git2::{DiffOptions, Repository};

use crate::core::diff::DiffSummary;
use crate::core::string::ToStrLossyOwned;

pub fn is_merge_active(repo: &Repository) -> bool {
  use git2::RepositoryState::*;
  matches!(repo.state(), Merge)
}

pub fn is_rebase_active(repo: &Repository) -> bool {
  use git2::RepositoryState::*;
  matches!(
    repo.state(),
    Rebase | RebaseInteractive | RebaseMerge | ApplyMailboxOrRebase
  )
}

pub fn is_pick_active(repo: &Repository) -> bool {
  use git2::RepositoryState::*;
  matches!(repo.state(), CherryPick | CherryPickSequence)
}

pub fn is_revert_active(repo: &Repository) -> bool {
  use git2::RepositoryState::*;
  matches!(repo.state(), Revert | RevertSequence)
}

/// Whether any state with possible conflicts is active
pub fn is_conflictable_active(repo: &Repository) -> bool {
  is_merge_active(repo) || is_rebase_active(repo) || is_pick_active(repo) || is_revert_active(repo)
}

/// Builds a [DiffSummary] of currently staged changes (HEAD vs. index).
/// Performs a similarity search on the diff. Filters out conflicted changes.
pub fn get_staged_changes(repo: &Repository) -> Result<DiffSummary> {
  let mut diff = repo.diff_tree_to_index(Some(&repo.head()?.peel_to_tree()?), None, None)?;
  diff.find_similar(None)?;

  let summary = DiffSummary::new(&diff)?.non_conflicts();
  Ok(summary)
}

/// Builds a [DiffSummary] of currently unstaged changes (index vs. workdir).
/// Performs a similarity search on the diff. Filters out conflicted changes.
///
/// # Params
/// - `untracked` - whether to include untracked files in the summary
pub fn get_unstaged_changes(repo: &Repository, untracked: bool) -> Result<DiffSummary> {
  let mut opts = DiffOptions::new();
  opts.include_untracked(untracked);

  let mut diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
  diff.find_similar(None)?;

  let summary = DiffSummary::new(&diff)?.non_conflicts();
  Ok(summary)
}

pub struct Conflict {
  pub path: String,
  pub kind: ConflictKind,
}

impl Display for Conflict {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{} {}", self.kind, self.path)
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConflictKind {
  ModifiedByBoth,

  AddedByUs,
  AddedByThem,
  AddedByBoth,

  DeletedByUs,
  DeletedByThem,
}

impl Display for ConflictKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", match self {
      ConflictKind::ModifiedByBoth => format!(
        "{}{}{}",
        style("M").yellow(),
        style("|").dim(),
        style("M").yellow(),
      ),

      ConflictKind::AddedByUs => format!(
        "{}{}{}",
        style("A").green(),
        style("|").dim(),
        style("-").dim(),
      ),

      ConflictKind::AddedByThem => format!(
        "{}{}{}",
        style("-").dim(),
        style("|").dim(),
        style("A").green(),
      ),

      ConflictKind::AddedByBoth => format!(
        "{}{}{}",
        style("A").green(),
        style("|").dim(),
        style("A").green(),
      ),

      ConflictKind::DeletedByUs => format!(
        "{}{}{}",
        style("D").red(),
        style("|").dim(),
        style("-").dim(),
      ),

      ConflictKind::DeletedByThem => format!(
        "{}{}{}",
        style("-").dim(),
        style("|").dim(),
        style("D").green(),
      ),
    })
  }
}

/// Gets a list of conflicted files currently in the index.
pub fn get_conflicts(repo: &Repository) -> Result<Vec<Conflict>> {
  let index = repo.index()?;
  let conflicts = index.conflicts()?;

  let mut out = Vec::new();

  for conflict in conflicts {
    let conflict = conflict?;

    // match against the existence of the file in each index
    let kind = match (&conflict.ancestor, &conflict.our, &conflict.their) {
      (Some(_), Some(_), Some(_)) => ConflictKind::ModifiedByBoth,
      (None, Some(_), None) => ConflictKind::AddedByUs,
      (None, None, Some(_)) => ConflictKind::AddedByThem,
      (Some(_), None, Some(_)) => ConflictKind::DeletedByUs,
      (Some(_), Some(_), None) => ConflictKind::DeletedByThem,
      (None, Some(_), Some(_)) => ConflictKind::AddedByBoth,

      // other combinations don't occur during conflicts
      _ => ConflictKind::ModifiedByBoth,
    };

    let path = conflict
      .our
      .as_ref()
      .or(conflict.their.as_ref())
      .or(conflict.ancestor.as_ref())
      .map(|entry| entry.path.to_str_lossy_owned())
      .context("Failed to find path of conflict entry")?;

    out.push(Conflict { path, kind });
  }

  Ok(out)
}

/// Gets conflicted, staged, and unstaged changes, and builds a printable
/// output.
///
/// # Params
/// - `untracked` - whether to include untracked files in the unstaged section
pub fn display_file_statuses(repo: &Repository, untracked: bool) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::new();
  let mut first_paragraph = true;

  let conflicts = get_conflicts(repo)?;
  if !conflicts.is_empty() {
    first_paragraph = false;

    write!(
      out,
      "{} - {} files",
      style("Conflicts").yellow(),
      style(conflicts.len()).cyan()
    )?;

    for conflict in conflicts {
      write!(out, "\n  {}", conflict)?;
    }
  } else if is_conflictable_active(repo) {
    // state that could have conflicts, but there are currently no conflicts
    write!(
      out,
      "{} - {}",
      style("Conflicts").yellow(),
      style("none").green()
    )?;
  }

  let staged = get_staged_changes(repo)?;
  if staged.num_files != 0 {
    if !first_paragraph {
      write!(out, "\n\n")?;
      first_paragraph = false;
    }
    write!(out, "{} - {}", style("Staged").green(), staged)?;
  }

  let unstaged = get_unstaged_changes(repo, untracked)?;
  if unstaged.num_files != 0 {
    if !first_paragraph {
      write!(out, "\n\n")?;
    }
    write!(out, "{} - {}", style("Unstaged").red(), unstaged)?;
  }

  Ok(out)
}
