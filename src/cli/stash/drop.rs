use anyhow::{Context, Result};
use console::style;

use crate::App;
use crate::util::stash::{display_stash_spec, parse_stash_spec};
use crate::util::string::ToStrLossy;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Drops a stash entry without applying it")]
pub struct DropArgs {
  /// Stash-spec to drop
  #[arg(value_name = "STASH_SPEC")]
  spec: Option<String>,
}

impl DropArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (branch, num) = parse_stash_spec(repo, self.spec.as_deref())?;
    let stash_refname = format!("refs/feature/stashes/{}", branch.name());
    let mut reflog = repo.reflog(&stash_refname)?;

    let commit_id = reflog
      .get(num)
      .with_context(|| format!("Entry {} does not exist", num))?
      .id_new();

    let stash = repo.find_commit(commit_id)?;

    reflog
      .remove(num, true)
      .context("Failed to remove stash entry")?;
    reflog.write()?;

    // if that was the only entry, delete the entire reflog and ref
    if reflog.is_empty() {
      let mut stash_ref = repo.find_reference(&stash_refname)?;
      stash_ref
        .delete()
        .context("Failed to clean up stash reference after dropping entry")?; // automatically deletes reflog
    }

    println!(
      "{} {}: {}",
      style("Dropped").red(),
      display_stash_spec(branch.name(), num),
      stash.message_bytes().to_str_lossy()
    );

    Ok(())
  }
}
