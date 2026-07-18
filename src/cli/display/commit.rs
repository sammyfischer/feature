use anyhow::{Context, Result};
use console::style;
use git2::Commit;

use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::display::{display_hash, display_signature};
use crate::core::string::ToStrLossy;
use crate::core::trim_hash;
use crate::core::user_config::{CommitMessageLevel, UserConfig};
use crate::{if_nerdfont, style};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayCommitOptions {
  pub time: DisplayTimeOptions,
  pub message: CommitMessageLevel,
}

/// Displays formatted info about a commit
///
/// Example:
/// ```txt
/// 1234567 Apr 14, 2025 at 5:46 PM by Author Name
///
///   subject
///
///   body
/// ```
pub fn display_commit(commit: &Commit, options: &DisplayCommitOptions) -> Result<String> {
  use std::fmt::Write;
  // around 60 chars for hash/time/author, another 80 for message (most of the
  // time this will only be a subject line)
  let mut out = String::with_capacity(140);

  // hash
  write!(out, "{}", display_hash(commit.as_object())?)?;

  // timestamp
  write!(
    out,
    " {}",
    style(display_time(&commit.time(), &options.time)?).magenta()
  )?;

  // author
  let author = commit.author();
  let committer = commit.committer();
  write!(out, " by {}", display_signature(Some(&commit.author())))?;

  if author.name_bytes() != committer.name_bytes() {
    write!(
      out,
      "\n  {} {}",
      style("Committed by").dim(),
      style(display_signature(Some(&commit.committer()))).dim()
    )?;
  }

  match options.message {
    CommitMessageLevel::None => {}

    CommitMessageLevel::Subject => write!(
      out,
      "\n\n  {}",
      commit
        .summary_bytes()
        .context("Failed to get commit subject")?
        .to_str_lossy()
    )?,

    CommitMessageLevel::Full => {
      // write each line tabbed by 2 spaces
      writeln!(out)?;
      for line in commit.message_bytes().to_str_lossy().lines() {
        write!(out, "\n  {}", line)?;
      }
    }
  };

  Ok(out)
}

/// A very concise format meant to be displayed on one line (although not
/// guaranteed to be).
///
/// ```txt
/// abcd123 Author Name, 5 minutes ago · implemented change
/// ```
///
/// The hash is yellow, all the remaining text is gray. Respects the user's
/// config for timestamp formatting and nerdfont.
///
/// # Params
/// - `hash` - Whether to display the hash. When false, the nerdfont commit icon
///   will also be hidden.
pub fn display_commit_compact(commit: &Commit, config: &UserConfig, hash: bool) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::with_capacity(80);

  if hash {
    write!(
      out,
      "{} ",
      style!(
        "{}{}",
        if_nerdfont!(config.nerd_font()?, " "),
        trim_hash(commit.as_object())?
      )
      .yellow()
    )?;
  }

  write!(
    out,
    "{}",
    style!(
      "{}, {} · {}",
      commit.author().name()?,
      display_time(&commit.time(), &DisplayTimeOptions {
        relative: config.format_relative()?,
        fmt: config.format_date()?
      })?,
      commit.summary()?.unwrap_or(commit.message()?)
    )
    .dim()
  )?;

  Ok(out)
}
