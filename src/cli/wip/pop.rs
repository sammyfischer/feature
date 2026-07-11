use anyhow::{Context, Result};
use console::style;
use git2::build::CheckoutBuilder;
use git2::{DiffOptions, IndexAddOption, Repository, Tree};

use crate::App;
use crate::core::status::display_file_statuses;
use crate::core::string::ToStrLossy;
use crate::core::user_config::UserConfig;
use crate::core::wip::{display_wip_spec, get_wip_refname, parse_wip_spec};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Applies and drops a wip entry")]
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

    let (branch, num) = parse_wip_spec(repo, self.spec.as_deref())?;
    let wip_refname = get_wip_refname(branch.name());
    let mut reflog = repo.reflog(&wip_refname)?;

    let wip = {
      let id = reflog
        .get(num)
        .context("There are no wip entries!")?
        .id_new();
      repo.find_commit(id)?
    };

    let parent = wip
      .parent(0)
      .context("Failed to get first parent of wip commit")?;

    let workdir = self
      .get_workdir_tree(repo)
      .context("Failed to build tree from workdir")?;
    let mut merge = repo.merge_trees(&parent.tree()?, &workdir, &wip.tree()?, None)?;

    if merge.has_conflicts() {
      let mut checkout = CheckoutBuilder::new();
      checkout.force();
      repo.checkout_index(Some(&mut merge), Some(&mut checkout))?;

      println!(
        "{} {} with conflicts: {}",
        style("Applied").yellow(),
        display_wip_spec(branch.name(), num),
        wip.message_bytes().to_str_lossy()
      );
      println!("{}", style("(wip entry was kept)").dim());
    } else {
      let merged_tree = {
        let id = merge.write_tree_to(repo)?;
        repo.find_tree(id)?
      };

      let mut checkout = CheckoutBuilder::new();
      checkout.force();
      repo.checkout_tree(merged_tree.as_object(), Some(&mut checkout))?;

      if !self.keep {
        // remove wip entry if apply was successful
        reflog
          .remove(num, true)
          .context("Failed to remove wip entry")?;
        reflog.write()?;

        // if that was the only entry, delete the entire reflog and ref
        if reflog.is_empty() {
          let mut wip_ref = repo.find_reference(&wip_refname)?;
          wip_ref.delete()?; // automatically deletes reflog
        }
      }

      println!(
        "{} {}: {}",
        style("Popped").green(),
        display_wip_spec(branch.name(), num),
        wip.message_bytes().to_str_lossy()
      );
    }

    let show_untracked = UserConfig::new(repo)?.status_untracked()?;

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
