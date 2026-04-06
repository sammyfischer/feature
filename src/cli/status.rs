use std::borrow::Cow;

use anyhow::{Context, Result};
use console::{Term, style, truncate_str};
use git2::DiffOptions;

use crate::cli::diff::display_diff_summary;
use crate::cli::{Cli, get_term_width};
use crate::{lossy, open_repo};

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "st",
  about = "View current status (current branch, author info, changes)"
)]
pub struct Args {
  /// Hides untracked files from output
  #[arg(short = 'u', long)]
  pub hide_untracked: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();
    let mut out = String::new();

    // HEAD info
    let head = repo.head().context("Failed to find HEAD reference")?;
    let commit = head
      .peel_to_commit()
      .context("Failed to get commit at HEAD")?;

    out.push_str(&format!(
      "{} -> {}",
      if head.is_branch() {
        style(lossy!(head.shorthand_bytes())).blue()
      } else {
        style(Cow::Borrowed("Detached HEAD")).red()
      },
      style(&commit.id().to_string()[..7]).yellow()
    ));

    // commit message
    if let Some(it) = commit.summary_bytes() {
      let msg = lossy!(it);
      let line = if Term::stdout().is_term() {
        truncate_str(&msg, get_term_width(), &style("\u{2026}").dim().to_string())
      } else {
        msg
      };
      out.push(' ');
      out.push_str(&line);
    }

    // end first line
    println!(
      "{}",
      if Term::stdout().is_term() {
        truncate_str(&out, get_term_width(), &style("\u{2026}").dim().to_string())
      } else {
        Cow::Borrowed(&*out)
      }
    );

    // signature/author info
    println!("{}", match repo.signature() {
      Ok(it) => {
        let name = lossy!(it.name_bytes());
        let email = lossy!(it.email_bytes());
        format!("{} {}", style(name).cyan(), style(email).dim())
      }
      Err(_) => style("No author info").red().to_string(),
    });

    let tree = commit.tree().ok();

    // staged changes
    let diff = repo
      .diff_tree_to_index(tree.as_ref(), None, None)
      .context("Failed to get staged chagnes")?;

    let stats = diff.stats().context("Failed to get staged changes")?;

    // if there are changes to print
    if stats.files_changed() != 0 {
      println!();
      print!("{} - ", style("Staged").green());
      print!("{}", match display_diff_summary(diff) {
        Ok(it) => it,
        Err(_) => "Failed to get summary of staged changes".to_string(),
      });
    }

    // unstaged changes
    let mut opts = if self.hide_untracked || cli.config.hide_untracked {
      None
    } else {
      let mut opts = DiffOptions::new();
      opts.include_untracked(true);
      Some(opts)
    };

    let diff = repo
      .diff_index_to_workdir(None, opts.as_mut())
      .context("Failed to get unstaged changes")?;

    let stats = diff.stats().context("Failed to get unstaged changes")?;

    // if there are changes to print
    if stats.files_changed() != 0 {
      println!();
      print!("{} - ", style("Unstaged").red());
      print!("{}", match display_diff_summary(diff) {
        Ok(it) => it,
        Err(_) => "Failed to get summary of unstaged changes".to_string(),
      });
    }

    Ok(())
  }
}
