use anyhow::Result;
use console::style;

use crate::App;
use crate::cli::wip::display_wipspec;
use crate::core::wip::WipList;

const LONG_ABOUT: &str = r#"Moves wips between branches.

The source can be specified as a wip spec, and defaults to the most recent wip
on the current branch.

The destination must always be specified. Wips can only be moved to the top of
another wip list, therefore the destination is a branch, not a wip spec."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "mv",
  about = "Moves wips between branches",
  long_about = LONG_ABOUT
)]
pub struct MoveArgs {
  /// The wip to move. Defaults to the most recent one on the current branch.
  #[arg(short = 's', long, visible_aliases = ["source", "src"], value_name = "WIP_SPEC")]
  from: Option<String>,

  /// The destination branch. This is a branch name, not a wip spec
  #[arg(value_name = "BRANCH")]
  to: String,
}

impl MoveArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (mut src_list, index) = WipList::parse_wipspec(repo, self.from.as_deref())?;
    let src_wip = src_list.remove(repo, index)?;
    let src_commit = repo.find_commit(src_wip.commit())?;

    let mut dst_list = WipList::from_branch(repo, self.to.clone())?;
    dst_list.push_commit(repo, &src_commit, Some(src_wip.message()))?;

    println!(
      "{} {} -> {}",
      style("Moved").green(),
      display_wipspec(src_list.branch(), index),
      display_wipspec(dst_list.branch(), 0)
    );

    Ok(())
  }
}
