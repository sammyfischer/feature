use anyhow::Result;
use console::style;

use crate::App;
use crate::cli::wip::display_wipspec;
use crate::core::wip::WipList;

#[derive(clap::Args, Clone, Debug)]
#[command(visible_alias = "rm", about = "Drops a wip without applying it")]
pub struct DropArgs {
  /// The wip-spec to drop
  #[arg(value_name = "WIP_SPEC")]
  spec: Option<String>,
}

impl DropArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (mut list, index) = WipList::parse_wipspec(repo, self.spec.as_deref())?;
    let wip = list.remove(repo, index)?;

    println!(
      "{} {}: {}",
      style("Dropped").red(),
      display_wipspec(list.branch(), index),
      wip.message()
    );

    Ok(())
  }
}
