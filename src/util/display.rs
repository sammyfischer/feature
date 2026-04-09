//! Helper functions to display formatted strings

use anyhow::{Context, Result};
use console::style;
use git2::{Delta, Diff, DiffLineType, Oid, Signature};

use crate::lossy;

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

pub fn trim_hash(id: &Oid) -> String {
  id.to_string()[..7].to_string()
}

pub fn display_hash(id: &Oid) -> String {
  style(trim_hash(id)).yellow().to_string()
}

/// Displays the name in cyan, email in dim (gray), and "no one" in red if there is no configured
/// signature. Errors if any error (other than not having a signature) is encountered.
pub fn display_signature(signature: Option<&Signature>) -> String {
  match signature {
    Some(it) => {
      let name = lossy!(it.name_bytes());
      let email = lossy!(it.email_bytes());
      format!("{} {}", style(name).cyan(), style(email).dim())
    }
    None => style("no one").red().to_string(),
  }
}

pub fn display_diff_summary_header(diff: &Diff) -> Result<String> {
  let stats = diff.stats().context("Failed to get diff stats")?;
  Ok(format!(
    "{} {} changed {}{}{}",
    style(stats.files_changed()).cyan(),
    if stats.files_changed() == 1 {
      "file"
    } else {
      "files"
    },
    style("[").dim(),
    display_plus_minus(stats.insertions(), stats.deletions()),
    style("]").dim()
  ))
}

/// Builds a pretty output to summarize the changes of this diff.
///
/// This displays each file that was changed, what type of change it was (created, modified, etc.),
/// and an insertion/deletion count. It also prints a summary line with the total number of files
/// changed and total insertions/deletions.
///
/// A newline is included at the end of the string, so you'll most often use this with [print!]
pub fn display_diff_summary(diff: &Diff) -> Result<String> {
  let mut out = String::new();

  // summary
  out.push_str(&display_diff_summary_header(diff)?);

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
      "\n  {} {} {}",
      file.status,
      file.name,
      display_plus_minus(file.insertions, file.deletions),
    ));
  }

  Ok(out)
}

/// Displays two numbers like `+p -m` where the first part is green and the second part is red.
///
/// The numbers are passed in as a tuple, where the first number is the plus and second is the
/// minus.
///
/// This is used to print ahead/behind and insertions/deletions.
pub fn display_plus_minus(plus: usize, minus: usize) -> String {
  format!(
    "{} {}",
    style(format!("+{}", plus)).green(),
    style(format!("-{}", minus)).red()
  )
}
