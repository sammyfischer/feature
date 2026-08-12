use console::style;
use git2::Delta;

use crate::cli::display::display_plus_minus;
use crate::core::diff::{DiffFileSummary, DiffSummary};
use crate::{dim_brackets, if_nerdfont};

/// Default display format for the header line. Shows number of files changed
/// and total insertions/deletions
pub fn display_summary_header(summary: &DiffSummary) -> String {
  format!(
    "{} {} changed {}",
    style(summary.num_files).cyan(),
    if summary.num_files == 1 {
      "file"
    } else {
      "files"
    },
    dim_brackets!(
      "{}",
      display_plus_minus(summary.insertions, summary.deletions)
    )
  )
}

pub fn display_summary(summary: &DiffSummary, nerdfont: bool) -> String {
  use std::fmt::Write;
  let mut out = String::new();

  write!(out, "{}", display_summary_header(summary)).unwrap();
  for file in &summary.files {
    write!(out, "\n  {}", display_file_summary(file, nerdfont)).unwrap();
  }

  out
}

/// Display a diff summary with a custom header line
pub fn display_summary_with_header(summary: &DiffSummary, header: &str, nerdfont: bool) -> String {
  use std::fmt::Write;
  let mut out = String::new();

  write!(out, "{}", header).unwrap();
  for file in &summary.files {
    write!(out, "\n  {}", display_file_summary(file, nerdfont)).unwrap();
  }

  out
}

pub fn display_file_summary(file: &DiffFileSummary, nerdfont: bool) -> String {
  match file.status {
    Delta::Unmodified => format!("{} {}", style("=").dim(), file.name),

    Delta::Added => format!(
      "{} {} {}",
      style(if_nerdfont!(nerdfont, "", "A")).green(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Deleted => format!(
      "{} {} {}",
      style(if_nerdfont!(nerdfont, "", "D")).red(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Modified => format!(
      "{} {} {}",
      style(if_nerdfont!(nerdfont, "", "M")).yellow(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Untracked => format!(
      "{} {}",
      style(if_nerdfont!(nerdfont, "󰘓", "U")).cyan(),
      file.name
    ),

    Delta::Conflicted => format!(
      "{} {}",
      style(if_nerdfont!(nerdfont, "󰩌", "X")).red(),
      file.name
    ),

    Delta::Renamed => format!(
      "{} {} -> {} {}",
      style(if_nerdfont!(nerdfont, "", "R")).magenta(),
      file.similar_old,
      file.name,
      // renames may have changes depending on the rename threshold
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Copied => format!(
      "{} {} -> {} {}",
      style(if_nerdfont!(nerdfont, "", "C")).magenta(),
      file.similar_old,
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Ignored => format!("{} {}", style("I").dim(), file.name),

    Delta::Typechange => format!(
      "{} {} {}",
      style("T").yellow(),
      file.name,
      display_plus_minus(file.insertions, file.deletions)
    ),

    Delta::Unreadable => format!("{} {}", style("?").red(), file.name),
  }
}
