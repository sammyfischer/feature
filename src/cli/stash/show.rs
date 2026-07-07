use anyhow::{Context, Result, anyhow};
use console::style;

use crate::App;
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::DiffSummary;
use crate::util::string::ToStrLossy;
use crate::util::term::paginate;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Display info about a stash entry")]
pub struct ShowArgs {
  /// Which stash to show
  index: Option<usize>,
}

impl ShowArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;
    let head = repo.head()?;

    if !head.is_branch() {
      return Err(anyhow!("Not currently on a branch"));
    }

    let index = self.index.unwrap_or(0);

    let branch = BranchMeta::from_reference(&head.resolve()?)?;
    let stash_refname = format!("refs/feature/stashes/{}", branch.name());
    let reflog = repo.reflog(&stash_refname)?;

    let commit_id = reflog
      .get(index)
      .with_context(|| format!("Entry {} does not exist", index))?
      .id_new();

    let stash = repo.find_commit(commit_id)?;
    let parent = stash
      .parent(0)
      .expect("Failed to get first parent of stash commit");

    let mut diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&stash.tree()?), None)?;
    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;

    use std::fmt::Write;
    let mut out = String::new();

    writeln!(
      out,
      "Stash {}{}{}",
      style(branch.name()).cyan(),
      style(":").dim(),
      style(index).cyan()
    )?;

    writeln!(out, "\n{}\n", summary)?;

    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
      writeln!(out, "{}", line.content().to_str_lossy()).is_ok()
    })?;

    paginate(out.as_bytes())?;
    Ok(())
  }
}
