use anyhow::{Context, Result};

use crate::App;
use crate::cli::display::diff::display_summary;
use crate::cli::term::paginate;
use crate::cli::wip::display_wipspec;
use crate::core::diff::{DiffSummary, get_formatted_diff};
use crate::core::user_config::UserConfig;
use crate::core::wip::WipList;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Display info about a wip")]
pub struct ShowArgs {
  /// The wip-spec to show
  #[arg(value_name = "WIP_SPEC")]
  spec: Option<String>,
}

impl ShowArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (list, index) = WipList::parse_wipspec(repo, self.spec.as_deref())?;
    let wip = list
      .get(index)
      .with_context(|| format!("Entry {} does not exist", index))?;

    let diff = wip.changes(repo)?;
    let summary = DiffSummary::new(&diff)?;

    let mut out: Vec<u8> = Vec::new();

    out.extend_from_slice(
      format!(
        "{}\n\n{}\n",
        display_wipspec(list.branch(), index),
        display_summary(&summary, UserConfig::new(repo)?.nerdfont()?)
      )
      .as_bytes(),
    );
    out.extend_from_slice(&get_formatted_diff(&diff)?);

    paginate(&out)?;
    Ok(())
  }
}
