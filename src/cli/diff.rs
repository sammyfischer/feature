//! Diff related functionality (not a subcommand)

use anyhow::{Context, Result};
use console::style;
use git2::{Delta, Diff, DiffLineType};

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

/// Builds a pretty output to summarize the changes of this diff.
///
/// This displays each file that was changed, what type of change it was (created, modified, etc.),
/// and an insertion/deletion count. It also prints a summary line with the total number of files
/// changed and total insertions/deletions.
///
/// A newline is included at the end of the string, so you'll most often use this with [print!]
pub fn display_diff_summary(diff: Diff) -> Result<String> {
  let mut out = String::new();
  let stats = diff.stats().context("Failed to get diff stats")?;

  // summary
  out.push_str(&format!(
    "{} {} changed [{} {}]\n",
    style(stats.files_changed()).cyan(),
    if stats.files_changed() == 1 {
      "file"
    } else {
      "files"
    },
    style(format!("+{}", stats.insertions())).green(),
    style(format!("-{}", stats.deletions())).red()
  ));

  // per-file info
  struct FileChanges {
    status: String,
    name: String,
    insertions: usize,
    deletions: usize,
  }
  let mut files: Vec<FileChanges> = Vec::new();
  // we need a mutable pointer to access `files` in multiple callbacks, but since these callbacks
  // are synchronous it's fine
  let files_ptr: *mut Vec<FileChanges> = &mut files;

  diff.foreach(
    &mut |delta, _| {
      let (status, name) = match delta.status() {
        Delta::Untracked => (style("U").cyan(), delta_filename!(delta, new_file)),
        Delta::Added => (style("A").green(), delta_filename!(delta, new_file)),
        Delta::Deleted => (style("D").red(), delta_filename!(delta, old_file)),
        Delta::Modified => (style("M").yellow(), delta_filename!(delta, new_file)),

        Delta::Renamed => (
          style("R").yellow(),
          format!(
            "{} -> {}",
            delta_filename!(delta, old_file),
            delta_filename!(delta, new_file),
          ),
        ),

        Delta::Copied => (
          style("C").green(),
          format!(
            "{} -> {}",
            delta_filename!(delta, old_file),
            delta_filename!(delta, new_file),
          ),
        ),
        _ => (style("?").dim(), delta_filename!(delta, new_file)),
      };

      unsafe { &mut *files_ptr }.push(FileChanges {
        status: status.to_string(),
        name,
        insertions: 0,
        deletions: 0,
      });
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

  for file in &files {
    out.push_str(&format!(
      "  {} {} {} {}\n",
      file.status,
      file.name,
      style(format!("+{}", file.insertions)).green(),
      style(format!("-{}", file.deletions)).red()
    ));
  }

  Ok(out)
}
