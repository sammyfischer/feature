use anyhow::{Result, anyhow};
use console::style;

use crate::App;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::wip::display_wip;
use crate::core::string::ToStrLossyOwned;
use crate::core::user_config::UserConfig;
use crate::core::wip::WipList;

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
    let config = UserConfig::new(repo)?;

    if self.all {
      // every wip ref
      let refs = repo.references_glob(&format!("{}/*", WipList::NAMESPACE))?;

      for rf in refs {
        let rf = rf?;
        let list = WipList::from_reference(repo, &rf)?;
        println!("{}", self.display_wip_list(&config, &list)?);
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

      let list = WipList::from_branch(repo, branch_name)?;
      println!("{}", self.display_wip_list(&config, &list)?);
    }

    Ok(())
  }

  fn display_wip_list(&self, config: &UserConfig, list: &WipList) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();

    let mut first = true;
    for wip in list.iter() {
      if first {
        first = false;
      } else {
        writeln!(out)?;
      }

      let time = wip.time();
      let msg = wip.message()?;

      write!(
        out,
        "{} {} {}",
        display_wip(&wip),
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
