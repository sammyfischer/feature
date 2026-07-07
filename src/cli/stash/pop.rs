use anyhow::{Context, Result, anyhow};
use console::style;
use git2::ErrorCode;

use crate::App;
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::DiffSummary;
use crate::util::string::ToStrLossy;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Applies and drops a stash entry")]
pub struct PopArgs {
  /// Don't drop the stash entry
  #[arg(short, long)]
  keep: bool,

  /// Which stash to pop
  index: Option<usize>,
}

impl PopArgs {
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
      .context("There are no stash entries!")?
      .id_new();

    let stash = repo.find_commit(commit_id)?;
    let parent = stash
      .parent(0)
      .context("Failed to get first parent of stash commit")?;

    let diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&stash.tree()?), None)?;

    // TODO: force apply, leaving conflicts, and don't drop entry if there are
    match repo.apply(&diff, git2::ApplyLocation::WorkDir, None) {
      Ok(_) => {
        if !self.keep {
          // remove stash entry if apply was successful
          reflog
            .remove(index, true)
            .context("Failed to remove stash entry")?;
          reflog.write()?;

          // if that was the only entry, delete the entire reflog and ref
          if reflog.is_empty() {
            let mut stash_ref = repo.find_reference(&stash_refname)?;
            stash_ref.delete()?; // automatically deletes reflog
          }
        }
      }
      Err(e) if e.code() == ErrorCode::ApplyFail => return Err(anyhow!("Failed to apply stash")),
      Err(e) => return Err(anyhow!(e)),
    };

    println!(
      "{} stash entry {}{}{}",
      style("Popped").green(),
      style(branch.name()).cyan(),
      style(":").dim(),
      style(index).cyan()
    );

    println!("{}", stash.message_bytes().to_str_lossy());

    let old_tree = repo.head()?.peel_to_tree()?;

    let mut staged = repo.diff_tree_to_index(Some(&old_tree), None, None)?;
    staged.find_similar(None)?;

    let staged = DiffSummary::new(&staged)?;
    if staged.num_files > 0 {
      println!("\n{} - {}", style("Staged").green(), staged);
    }

    let mut unstaged = repo.diff_index_to_workdir(None, None)?;
    unstaged.find_similar(None)?;

    let unstaged = DiffSummary::new(&unstaged)?;
    if unstaged.num_files > 0 {
      println!("\n{} - {}", style("Unstaged").red(), staged);
    }

    Ok(())
  }
}
