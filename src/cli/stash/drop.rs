use anyhow::{Context, Result, anyhow};
use console::style;

use crate::App;
use crate::util::branch_meta::BranchMeta;
use crate::util::string::ToStrLossy;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Drops a stash entry without applying it")]
pub struct DropArgs {
  /// Which stash to drop
  index: Option<usize>,
}

impl DropArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;
    let head = repo.head()?;

    if !head.is_branch() {
      return Err(anyhow!("Not currently on a branch"));
    }

    let index = self.index.unwrap_or(0);

    let branch = BranchMeta::from_reference(&head.resolve()?)?;
    let stash_refname = format!("refs/feature/stashes/{}", branch.name());
    let mut reflog = repo.reflog(&stash_refname)?;

    let commit_id = reflog
      .get(index)
      .with_context(|| format!("Entry {} does not exist", index))?
      .id_new();

    let stash = repo.find_commit(commit_id)?;

    reflog
      .remove(index, true)
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
      "{} stash entry {}{}{}",
      style("Dropped").red(),
      style(branch.name()).cyan(),
      style(":").dim(),
      style(index).cyan()
    );

    println!("{}", stash.message_bytes().to_str_lossy());

    Ok(())
  }
}
