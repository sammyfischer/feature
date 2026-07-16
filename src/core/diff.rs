//! Diff related helpers and display functions

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use git2::{Delta, Diff, DiffLineType};
use which::which;

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
  /// Iterates through the diff and summarizes the information into a new
  /// [DiffSummary]
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

    // we need a raw pointer to unsafely access `files` in multiple callbacks, but
    // since these callbacks are synchronous it's fine
    let files_ptr: *mut Vec<DiffFileSummary> = &mut summary.files;

    diff.foreach(
      &mut |delta, _| {
        let mut file = DiffFileSummary {
          status: delta.status(),
          // reasonable initial capacity for filenames
          name: String::with_capacity(40),
          similar_old: String::with_capacity(40),
          insertions: 0,
          deletions: 0,
        };

        match delta.status() {
          Delta::Unmodified
          | Delta::Untracked
          | Delta::Added
          | Delta::Modified
          | Delta::Ignored
          | Delta::Typechange
          | Delta::Unreadable
          | Delta::Conflicted => file.name.push_str(&delta_filename!(delta, new_file)),
          Delta::Deleted => file.name.push_str(&delta_filename!(delta, old_file)),
          Delta::Renamed | Delta::Copied => {
            file.similar_old.push_str(&delta_filename!(delta, old_file));
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

  /// Creates a new diff summary out of the non-conflicted files in this
  /// summary.
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
}

#[derive(Clone)]
pub struct DiffFileSummary {
  /// The type of change that occured to the file
  pub status: Delta,

  /// The name of the file. This is the old filename for delete, and the new
  /// name for everything else
  pub name: String,

  /// For similarity detection, this is the old name of the file. This is set
  /// for renames and copies
  pub similar_old: String,

  /// The number of line insertions. This is only meaningful for some statuses,
  /// but there will always be a value
  pub insertions: usize,

  /// The number of line deletions. This is only meaningful for some statuses,
  /// but there will always be a value
  pub deletions: usize,
}

/// Gets the bytes of a diff, possibly fitlering it through delta
pub fn get_formatted_diff(diff: &Diff) -> Result<Vec<u8>> {
  // collect diff output
  let mut bytes: Vec<u8> = Vec::new();
  diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
    let prefix = match line.origin_value() {
      DiffLineType::Context => ' '.to_string(),
      DiffLineType::Addition => '+'.to_string(),
      DiffLineType::Deletion => '-'.to_string(),
      _ => String::new(),
    };
    bytes.extend_from_slice(prefix.as_bytes());
    bytes.extend_from_slice(line.content());
    true
  })?;

  if let Ok(delta) = which("delta") {
    // pass bytes to delta if found, then return its output
    let mut cmd = Command::new(delta)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .spawn()?;

    if let Some(stdin) = &mut cmd.stdin {
      stdin.write_all(&bytes)?;
    }

    let out = cmd.wait_with_output()?;
    Ok(out.stdout)
  } else {
    // just return the bytes
    Ok(bytes)
  }
}
