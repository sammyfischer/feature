use git2::{BranchType, Repository};

use crate::cli::{Cli, CliResult, fetch_all, get_all_branches, get_current_branch, is_merged};
use crate::{await_child, cli_err, cli_err_fn, data, git};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  #[arg(long)]
  dry_run: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let repo = Repository::open_from_env()?;
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
  fn safe_delete_branch(&self, cli: &Cli, repo: &Repository, branch_name: &String) -> CliResult {
    // skip base branches
    if cli.config.bases.contains(branch_name) {
      return Err(cli_err!(
        Generic,
        "Cannot delete base branch {}",
        branch_name
      ));
    }

    // skip other protected branches
    if cli.config.protect.contains(branch_name) {
      return Err(cli_err!(
        Generic,
        "Cannot delete protected branch {}",
        branch_name
      ));
    }

    // skip current branch
    if get_current_branch(repo).is_ok_and(|it| &it == branch_name) {
      return Err(cli_err!(
        Generic,
        "Cannot delete currently checked-out branch {}",
        branch_name
      ));
    }

    let config = repo.config()?;

    // find base branch from db, or just use the trunk branch
    let base_name =
      data::get_feature_base(&config, branch_name).unwrap_or(cli.config.trunk.clone());

    // detect if branch is merged (i.e. has no commits that aren't on its base)
    let is_merged = is_merged(repo, branch_name, &base_name).map_err(cli_err_fn!(
      Generic,
      e,
      "Failed to determine if {} is merged into {}: {}",
      branch_name,
      base_name,
      e
    ))?;

    if is_merged {
      // in dry-run mode, print the branch name but don't delete
      if self.dry_run {
        println!("{}", branch_name);
        return Ok(());
      }

      let mut branch = repo
        .find_branch(branch_name, BranchType::Local)
        .map_err(cli_err_fn!(
          Git,
          e,
          "Failed to get reference to branch {}: {}",
          branch_name,
          e
        ))?;

      branch.delete().map_err(cli_err_fn!(
        Git,
        e,
        "Failed to delete {}: {}",
        branch_name,
        e
      ))?;

      // git2 can't remove entire config sections, but git provides a command to do so
      let key = format!("branch.{}", branch_name);
      let mut child = git!("config", "--remove-section", key).spawn()?;
      await_child!(child, "Failed to call git")?;
    }

    Ok(())
  }
}
