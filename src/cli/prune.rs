use anyhow::{Context, Result};
use console::style;
use git2::{BranchType, Repository};

use crate::cli::Cli;
use crate::util::branch::{fetch_all, get_all_branch_names, get_current_branch_name};
use crate::util::display::trim_hash;
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
    let branches = get_all_branch_names(&repo)?;

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
  /// - it's not the current branch
  /// - it's changes are merged into its base
  fn safe_delete_branch(&self, cli: &Cli, repo: &Repository, branch_name: &String) -> Result<()> {
    // skip base branches
    if cli.config.bases.contains(branch_name) {
      return Ok(());
    }

    // skip other protected branches
    if cli.config.protect.contains(branch_name) {
      return Ok(());
    }

    // skip current branch
    if &get_current_branch_name(repo).context("Failed to get current branch")? == branch_name {
      // not necessarily an error, but the user should know that a non-base non-protected branch was
      // skipped and may manually need to be deleted
      println!("Skipping currently checked-out branch: {}", branch_name);
      return Ok(());
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

      let commit = branch
        .get()
        .peel_to_commit()
        .with_context(|| format!("Failed to get commit pointed to by {}", branch_name))?;

      branch
        .delete()
        .with_context(|| format!("Failed to delete branch {}", branch_name))?;

      println!(
        "{} {} {}",
        style("Deleted").red(),
        branch_name,
        &style(&format!("(was {})", &trim_hash(&commit.id()))).dim()
      );

      // git2 can't remove entire config sections, but git provides a command to do so
      let key = format!("branch.{}", branch_name);
      let mut child = git!("config", "--remove-section", key)
        .spawn()
        .context("Failed to call git")?;

      await_child!(child, "Git")?;
    }

    Ok(())
  }
}

/// Whether branch is merged into base. A branch is considered merged if:
/// - it points to the same commit as its base
/// - it's not a descendant of base (i.e. there are no new commits)
fn is_merged(repo: &Repository, branch_name: &str, base_name: &str) -> Result<bool> {
  let branch = repo.revparse_single(branch_name)?;
  let base = repo.revparse_single(base_name)?;

  let branch_commit = branch.peel_to_commit()?.id();
  let base_commit = base.peel_to_commit()?.id();

  if branch_commit == base_commit {
    return Ok(true);
  }

  // whether branch is a descendant of base. if it is, then there are newer unmerged commits
  let is_descendant = repo.graph_descendant_of(branch_commit, base_commit)?;
  Ok(!is_descendant)
}
