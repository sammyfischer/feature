use console::style;
use git2::Delta;

use crate::cli::display::display_plus_minus;
use crate::core::diff::{DiffFileSummary, DiffSummary};

/// Default display format for the header line. Shows number of files changed
/// and total insertions/deletions
pub fn display_summary_header(summary: &DiffSummary) -> String {
  format!(
    "{} {} changed {}{}{}",
    style(summary.num_files).cyan(),
    if summary.num_files == 1 {
      "file"
    } else {
      "files"
    },
    style("[").dim(),
    display_plus_minus(summary.insertions, summary.deletions),
    style("]").dim()
  )
}

pub fn display_summary(summary: &DiffSummary) -> String {
  use std::fmt::Write;
  let mut out = String::new();

  write!(out, "{}", display_summary_header(summary)).unwrap();
  for file in &summary.files {
    write!(out, "\n  {}", display_file_summary(file)).unwrap();
  }

  out
}

pub fn display_file_summary(file: &DiffFileSummary) -> String {
  match file.status {
    Delta::Unmodified => format!("{} {}", style("=").dim(), file.name),

    Delta::Added => format!(
      "{} {} {}",
      style("A").green(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Deleted => format!(
      "{} {} {}",
      style("D").red(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Modified => format!(
      "{} {} {}",
      style("M").yellow(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Renamed => format!(
      "{} {} -> {} {}",
      style("R").magenta(),
      file.similar_old,
      file.name,
      // renames may have changes depending on the rename threshold
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Copied => format!(
      "{} {} -> {} {}",
      style("C").magenta(),
      file.similar_old,
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Ignored => format!("{} {}", style("I").dim(), file.name),
    Delta::Untracked => format!("{} {}", style("U").cyan(), file.name),

    Delta::Typechange => format!(
      "{} {} {}",
      style("T").yellow(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Unreadable => format!("{} {}", style("?").red(), file.name),
    Delta::Conflicted => format!("{} {}", style("X").red(), file.name),
  }
}
