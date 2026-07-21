use anyhow::{Context, Result};
use console::style;

use crate::App;
use crate::cli::status::display_file_statuses;
use crate::cli::wip::display_wipspec;
use crate::core::user_config::UserConfig;
use crate::core::wip::WipList;

#[derive(clap::Args, Clone, Debug)]
#[command(visible_alias = "apply", about = "Applies and drops a wip entry")]
pub struct PopArgs {
  /// Don't drop the wip entry
  #[arg(short, long)]
  keep: bool,

  /// The wip-spec to pop
  #[arg(value_name = "WIP_SPEC")]
  spec: Option<String>,
}

impl PopArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (mut list, index) = WipList::parse_wipspec(repo, self.spec.as_deref())?;
    let wip_msg = list
      .get(index)
      .with_context(|| format!("Entry {} does not exist", index))?
      .message()
      .to_string();

    let has_conflicts = list.apply(repo, index)?;
    if has_conflicts {
      println!(
        "{} {} with conflicts: {}",
        style("Applied").yellow(),
        display_wipspec(list.branch(), index),
        wip_msg
      );
      println!("{}", style("(wip entry was kept)").dim());
    } else {
      if !self.keep {
        // remove wip entry if apply was successful
        list.remove(repo, index)?;
      }

      println!(
        "{} {}: {}",
        style("Popped").green(),
        display_wipspec(list.branch(), index),
        wip_msg
      );
    }

    let config = UserConfig::new(repo)?;
    let nerdfont = config.nerdfont()?;
    let show_untracked = config.status_untracked()?;

    let statuses = display_file_statuses(repo, show_untracked, nerdfont)?;
    if !statuses.is_empty() {
      println!(
        "\n{}",
        display_file_statuses(repo, show_untracked, nerdfont)?
      );
    }

    Ok(())
  }
}
