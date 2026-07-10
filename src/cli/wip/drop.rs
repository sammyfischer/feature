use anyhow::{Context, Result};
use console::style;

use crate::App;
use crate::core::string::ToStrLossy;
use crate::core::wip::{display_wip_spec, get_wip_refname, parse_wip_spec};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Drops a wip without applying it")]
pub struct DropArgs {
  /// The wip-spec to drop
  #[arg(value_name = "WIP_SPEC")]
  spec: Option<String>,
}

impl DropArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (branch, num) = parse_wip_spec(repo, self.spec.as_deref())?;
    let wip_refname = get_wip_refname(branch.name());
    let mut reflog = repo.reflog(&wip_refname)?;

    let commit_id = reflog
      .get(num)
      .with_context(|| format!("Entry {} does not exist", num))?
      .id_new();

    let wip = repo.find_commit(commit_id)?;

    reflog.remove(num, true).context("Failed to remove wip")?;
    reflog.write()?;

    // if that was the only entry, delete the entire reflog and ref
    if reflog.is_empty() {
      let mut wip_ref = repo.find_reference(&wip_refname)?;
      wip_ref
        .delete()
        .context("Failed to clean up wip reference after dropping entry")?; // automatically deletes reflog
    }

    println!(
      "{} {}: {}",
      style("Dropped").red(),
      display_wip_spec(branch.name(), num),
      wip.message_bytes().to_str_lossy()
    );

    Ok(())
  }
}
