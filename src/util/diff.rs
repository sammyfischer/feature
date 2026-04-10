//! Diff related helpers and display functions

use std::fmt::Display;

use anyhow::{Context, Result};
use console::style;
use git2::{Delta, Diff, DiffLineType};

use crate::util::display::display_plus_minus;

macro_rules! delta_filename {
  ($delta:ident, $file:ident) => {
    $delta
      .$file()
      .path()
      .expect("Failed to get file path from delta")
      .display()
      .to_string()
  };
}

pub struct DiffSummary {
  /// Number of files changed
  pub num_files: usize,

  /// Total number of insertions
  pub insertions: usize,

  /// Total number of deletions
  pub deletions: usize,

  /// Stats for each file changed
  pub files: Vec<DiffFileSummary>,
}

impl DiffSummary {
  /// Iterates through the diff and summarizes the information into a new [DiffStats]
  pub fn new(diff: &Diff) -> Result<Self> {
    let mut summary = DiffSummary {
      num_files: 0,
      insertions: 0,
      deletions: 0,
      files: Vec::new(),
    };

    // summary
    let stats = diff.stats().context("Failed to get diff stats")?;
    summary.num_files = stats.files_changed();
    summary.insertions = stats.insertions();
    summary.deletions = stats.deletions();

    // we need a raw pointer to unsafely access `files` in multiple callbacks, but since these
    // callbacks are synchronous it's fine
    let files_ptr: *mut Vec<DiffFileSummary> = &mut summary.files;

    diff.foreach(
      &mut |delta, _| {
        let mut file = DiffFileSummary {
          status: delta.status(),
          name: String::new(),
          rename_old: String::new(),
          insertions: 0,
          deletions: 0,
        };

        match delta.status() {
          Delta::Unmodified
          | Delta::Untracked
          | Delta::Added
          | Delta::Modified
          | Delta::Copied
          | Delta::Ignored
          | Delta::Typechange
          | Delta::Unreadable
          | Delta::Conflicted => file.name.push_str(&delta_filename!(delta, new_file)),
          Delta::Deleted => file.name.push_str(&delta_filename!(delta, old_file)),
          Delta::Renamed => {
            file.rename_old.push_str(&delta_filename!(delta, old_file));
            file.name.push_str(&delta_filename!(delta, new_file));
          }
        };

        unsafe { &mut *files_ptr }.push(file);
        true
      },
      None,
      None,
      Some(&mut |_, _, line| {
        if let Some(file) = unsafe { &mut *files_ptr }.last_mut() {
          match line.origin_value() {
            DiffLineType::Addition => file.insertions += 1,
            DiffLineType::Deletion => file.deletions += 1,
            _ => {}
          }
        }
        true
      }),
    )?;

    Ok(summary)
  }

  /// Creates a new diff summary out of the conflicted files in this summary.
  ///
  /// `insertions` and `deletions` are always 0
  pub fn conflicts(&self) -> Self {
    let mut conflicted_files: Vec<DiffFileSummary> = Vec::new();
    for file in &self.files {
      if file.status == Delta::Conflicted {
        conflicted_files.push(file.clone());
      }
    }

    Self {
      num_files: conflicted_files.len(),
      insertions: 0,
      deletions: 0,
      files: conflicted_files,
    }
  }

  /// Creates a new diff summary out of the non-conflicted files in this summary.
  pub fn non_conflicts(&self) -> Self {
    let mut conflicted_files: Vec<DiffFileSummary> = Vec::new();
    for file in &self.files {
      if file.status != Delta::Conflicted {
        conflicted_files.push(file.clone());
      }
    }

    Self {
      num_files: conflicted_files.len(),
      // conflicted files always have 0, so we don't have to recount
      insertions: self.insertions,
      deletions: self.deletions,
      files: conflicted_files,
    }
  }

  /// Default display format for the header line. Shows number of files changed and total
  /// insertions/deletions
  pub fn display_header(&self) -> String {
    format!(
      "{} {} changed {}{}{}",
      style(self.num_files).cyan(),
      if self.num_files == 1 { "file" } else { "files" },
      style("[").dim(),
      display_plus_minus(self.insertions, self.deletions),
      style("]").dim()
    )
  }

  /// Displays a header similar to the default except the text says "n conflicted files". Assumes
  /// this summary only contains conflicted files.
  pub fn display_conflict_header(&self) -> String {
    let num = self.num_files;
    format!(
      "{} conflicted {}",
      style(num).cyan(),
      if num == 1 { "file" } else { "files" }
    )
  }

  /// Displays with the default format, but uses the conflict header. Assumes this summary contains
  /// only conflicted files.
  pub fn display_conflicts(&self) -> String {
    let mut out = self.display_conflict_header();
    for file in &self.files {
      out.push_str(&format!("\n  {}", file));
    }
    out
  }
}

impl Display for DiffSummary {
  /// Default format to display an entire summary. Shows the default header line, with each file in
  /// a row below it, tabbed over by two spaces
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.display_header())?;
    for file in &self.files {
      write!(f, "\n  {}", file)?;
    }
    Ok(())
  }
}

#[derive(Clone)]
pub struct DiffFileSummary {
  /// The type of change that occured to the file
  pub status: Delta,

  /// The name of the file. This is the old filename for delete, and the new name for everything
  /// else
  pub name: String,

  /// For renames only, this is the old name of the file
  pub rename_old: String,

  /// The number of line insertions. This is only meaningful for some statuses, but there will
  /// always be a value
  pub insertions: usize,

  /// The number of line deletions. This is only meaningful for some statuses, but there will
  /// always be a value
  pub deletions: usize,
}

impl Display for DiffFileSummary {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self.status {
      Delta::Unmodified => write!(f, "{} {}", style("=").dim(), self.name),

      Delta::Added => write!(
        f,
        "{} {} {}",
        style("A").green(),
        self.name,
        display_plus_minus(self.insertions, self.deletions)
      ),

      Delta::Deleted => write!(
        f,
        "{} {} {}",
        style("D").red(),
        self.name,
        display_plus_minus(self.insertions, self.deletions)
      ),

      Delta::Modified => write!(
        f,
        "{} {} {}",
        style("M").yellow(),
        self.name,
        display_plus_minus(self.insertions, self.deletions)
      ),

      Delta::Renamed => write!(
        f,
        "{} {} -> {} {}",
        style("R").yellow(),
        self.name,
        self.rename_old,
        // renames may have changes depending on the rename threshold
        display_plus_minus(self.insertions, self.deletions)
      ),

      Delta::Copied => write!(
        f,
        "{} {} {}",
        style("C").green(),
        self.name,
        display_plus_minus(self.insertions, self.deletions)
      ),

      Delta::Ignored => write!(f, "{} {}", style("I").dim(), self.name),
      Delta::Untracked => write!(f, "{} {}", style("U").cyan(), self.name),

      Delta::Typechange => write!(
        f,
        "{} {} {}",
        style("T").yellow(),
        self.name,
        display_plus_minus(self.insertions, self.deletions)
      ),

      Delta::Unreadable => write!(f, "{} {}", style("?").red(), self.name),
      Delta::Conflicted => write!(f, "{} {}", style("X").red(), self.name),
    }
  }
}
