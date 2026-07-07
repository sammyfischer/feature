use anyhow::{Result, anyhow};
use console::style;
use git2::{Reflog, Repository};

use crate::util::display::{DisplayTimeOptions, display_time};
use crate::util::string::{ToStrLossy, ToStrLossyOwned};
use crate::{App, data};

#[derive(clap::Args, Clone, Debug)]
#[command(visible_alias = "ls", about = "Lists stashes on branch")]
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

    let branch_name = head.shorthand_bytes().to_str_lossy();

    if self.all {
      let refs = repo.references_glob("refs/feature/stashes/*")?;

      for reference in refs {
        let reference = reference?;
        let reflog = repo.reflog(reference.name()?)?;

        println!("{}", self.display_stash_list(repo, &branch_name, &reflog)?);
      }
    } else {
      let reflog = repo.reflog(&format!("refs/feature/stashes/{}", &branch_name))?;
      println!("{}", self.display_stash_list(repo, &branch_name, &reflog)?);
    }

    Ok(())
  }

  fn display_stash_list(
    &self,
    repo: &Repository,
    branch_name: &str,
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
        style(branch_name).cyan(),
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
