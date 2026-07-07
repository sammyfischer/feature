use anyhow::{Result, anyhow};
use console::style;
use git2::{Reflog, Repository};

use crate::util::branch_meta::BranchMeta;
use crate::util::display::{DisplayTimeOptions, display_time};
use crate::util::string::ToStrLossyOwned;
use crate::{App, data};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Lists stashes on branch")]
pub struct ListArgs {
  /// List all stashes, not only this branch's stashes
  #[arg(short, long)]
  all: bool,
}

impl ListArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;
    let head = repo.head()?;

    if !head.is_branch() {
      return Err(anyhow!("Not currently on a branch"));
    }

    if self.all {
      let refs = repo.references_glob("refs/feature/stashes/*")?;

      for reference in refs {
        let reference = reference?;
        let branch = BranchMeta::from_reference(&reference)?;
        let reflog = repo.reflog(branch.refname())?;

        println!("{}", self.display_stash_list(repo, &branch, &reflog)?);
      }
    } else {
      let branch = BranchMeta::from_reference(&head.resolve()?)?;
      let reflog = repo.reflog(branch.refname())?;

      println!("{}", self.display_stash_list(repo, &branch, &reflog)?);
    }

    Ok(())
  }

  fn display_stash_list(
    &self,
    repo: &Repository,
    branch: &BranchMeta,
    reflog: &Reflog,
  ) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();
    let config = repo.config()?.snapshot()?;

    let mut first = true;
    for (i, entry) in reflog.iter().enumerate() {
      if first {
        first = false;
      } else {
        writeln!(out)?;
      }

      let time = entry.committer().when();

      let msg = match entry.message_bytes() {
        Some(bytes) => bytes.to_str_lossy_owned(),
        None => String::new(),
      };

      write!(
        out,
        "{}{}{} {} {}",
        style(branch.name()).cyan(),
        style(":").dim(),
        style(i).cyan(),
        style(display_time(&time, &DisplayTimeOptions {
          relative: data::get_format_relative(&config)?,
          fmt: data::get_format_date(&config)?,
        })?)
        .magenta(),
        msg
      )?;
    }

    Ok(out)
  }
}
