use anyhow::{Result, anyhow};
use console::style;
use git2::{Reflog, Repository};

use crate::App;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::core::string::ToStrLossyOwned;
use crate::core::user_config::UserConfig;
use crate::core::wip::{display_wip_spec, get_wip_refname};

#[derive(clap::Args, Clone, Debug)]
#[command(visible_alias = "ls", about = "Lists wips on branch")]
pub struct ListArgs {
  /// List all wips, not only this branch's wips
  #[arg(short, long)]
  all: bool,

  /// Branch whose wips will be listed
  branch: Option<String>,
}

impl ListArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    if self.all {
      let refs = repo.references_glob(&get_wip_refname("*"))?;

      for reference in refs {
        let reference = reference?;
        let name = reference.shorthand()?;
        let reflog = repo.reflog(reference.name()?)?;

        println!("{}", self.display_wip_list(repo, name, &reflog)?);
      }
    } else {
      let branch_name = match &self.branch {
        Some(name) => name.to_owned(),
        None => {
          let head = repo.head()?;
          if !head.is_branch() {
            return Err(anyhow!("Not currently on a branch"));
          }
          head.shorthand_bytes().to_str_lossy_owned()
        }
      };
      let reflog = repo.reflog(&get_wip_refname(&branch_name))?;
      println!("{}", self.display_wip_list(repo, &branch_name, &reflog)?);
    }

    Ok(())
  }

  fn display_wip_list(
    &self,
    repo: &Repository,
    branch_name: &str,
    reflog: &Reflog,
  ) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();
    let config = UserConfig::new(repo)?;

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
        "{} {} {}",
        display_wip_spec(branch_name, i),
        style(display_time(&time, &DisplayTimeOptions {
          relative: config.format_relative()?,
          fmt: config.format_date()?,
        })?)
        .magenta(),
        msg
      )?;
    }

    Ok(out)
  }
}
