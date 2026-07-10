use anyhow::{Context, Result, anyhow};
use console::style;
use git2::build::CheckoutBuilder;
use git2::{DiffOptions, IndexAddOption, Repository, Tree};

use crate::util::status::display_file_statuses;
use crate::util::string::ToStrLossy;
use crate::{App, data};

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

    let stash_refname = format!("refs/feature/stashes/{}", head.resolve()?.shorthand()?);
    let mut reflog = repo.reflog(&stash_refname)?;

    let stash = {
      let id = reflog
        .get(index)
        .context("There are no stash entries!")?
        .id_new();
      repo.find_commit(id)?
    };

    let parent = stash
      .parent(0)
      .context("Failed to get first parent of stash commit")?;

    let workdir = self
      .get_workdir_tree(repo)
      .context("Failed to build tree from workdir")?;
    let mut merge = repo.merge_trees(&parent.tree()?, &workdir, &stash.tree()?, None)?;

    if merge.has_conflicts() {
      let mut checkout = CheckoutBuilder::new();
      checkout.force();
      repo.checkout_index(Some(&mut merge), Some(&mut checkout))?;

      println!(
        "{} with conflicts: {}\n{}",
        style("Applied").yellow(),
        stash.message_bytes().to_str_lossy(),
        style("(stash entry was kept)").dim()
      );
    } else {
      let merged_tree = {
        let id = merge.write_tree_to(repo)?;
        repo.find_tree(id)?
      };

      let mut checkout = CheckoutBuilder::new();
      checkout.force();
      repo.checkout_tree(merged_tree.as_object(), Some(&mut checkout))?;

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

      println!(
        "{}: {}",
        style("Popped").green(),
        stash.message_bytes().to_str_lossy()
      );
    }

    let show_untracked = data::get_status_untracked(&repo.config()?.snapshot()?)?;

    let statuses = display_file_statuses(repo, show_untracked)?;
    if !statuses.is_empty() {
      println!("\n{}", display_file_statuses(repo, show_untracked)?);
    }

    Ok(())
  }

  /// Writes the entire workdir as a tree to the odb and returns it
  fn get_workdir_tree<'tree>(&self, repo: &'tree Repository) -> Result<Tree<'tree>> {
    let head_tree = repo.head()?.peel_to_tree()?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);

    // build index starting from head, then just adding all files in workdir (except
    // ignored)
    let mut index = repo.index()?;
    index.read_tree(&head_tree)?;
    index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
    let id = index.write_tree_to(repo)?;
    Ok(repo.find_tree(id)?)
  }
}
