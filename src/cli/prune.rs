use anyhow::{Context, Result, anyhow};
use git2::{BranchType, Repository};

use crate::cli::{Cli, fetch_all, get_all_branches, get_current_branch, is_merged};
use crate::{await_child, data, git, open_repo};

const LONG_ABOUT: &str = r"Deletes all branches that:
- have a known base branch
- are an ancestor of their base branch
- aren't a base or protected branch
- aren't the current branch

These checks should prevent most accidental deletions, and at least ensure that
any deleted branches were redundant (being an ancestor of the base means the
base contains the branch's commit history already).";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Deletes merged feature branches", long_about = LONG_ABOUT)]
pub struct Args {
  #[arg(long)]
  dry_run: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();
    fetch_all(&repo)?;

    // get list of all branches
    let branches = get_all_branches(&repo)?;

    if self.dry_run {
      println!("Deletion candidates:")
    }

    for branch_name in branches {
      if let Err(e) = self.safe_delete_branch(cli, &repo, &branch_name) {
        eprintln!("{}", e);
      }
    }

    Ok(())
  }

  /// Deletes a branch if:
  /// - it's not a base branch
  /// - it's not a protected branch
  /// - it's changes are merged into its base
  fn safe_delete_branch(&self, cli: &Cli, repo: &Repository, branch_name: &String) -> Result<()> {
    // skip base branches
    if cli.config.bases.contains(branch_name) {
      return Err(anyhow!("Cannot delete a base branch"));
    }

    // skip other protected branches
    if cli.config.protect.contains(branch_name) {
      return Err(anyhow!("Cannot delete a protected branch"));
    }

    // skip current branch
    if &get_current_branch(repo).context("Failed to get current branch")? == branch_name {
      return Err(anyhow!("Cannot delete currently checked-out branch"));
    }

    let config = repo.config().context("Failed to get git config")?;

    // find base branch from db, else skip
    let base_name = data::get_feature_base(&config, branch_name)
      .context("Cannot prune branches without a base")?;

    // detect if branch is merged (i.e. has no commits that aren't on its base)
    let is_merged = is_merged(repo, branch_name, &base_name).with_context(|| {
      format!(
        "Failed to determine if {} is merged into {}",
        branch_name, base_name
      )
    })?;

    if is_merged {
      // in dry-run mode, print the branch name but don't delete
      if self.dry_run {
        println!("{}", branch_name);
        return Ok(());
      }

      let mut branch = repo
        .find_branch(branch_name, BranchType::Local)
        .with_context(|| format!("Failed to get reference to branch {}", branch_name))?;

      branch
        .delete()
        .unwrap_or_else(|_| panic!("Failed to delete branch {}", branch_name));

      // git2 can't remove entire config sections, but git provides a command to do so
      let key = format!("branch.{}", branch_name);
      let mut child = git!("config", "--remove-section", key)
        .spawn()
        .context("Failed to call git")?;
      await_child!(child, "Failed to call git")?;
    }

    Ok(())
  }
}
