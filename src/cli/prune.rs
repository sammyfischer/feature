use git2::{BranchType, Repository};

use crate::cli::{Cli, CliResult, fetch_all, get_all_branches, get_current_branch, is_merged};
use crate::data;

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  #[arg(long)]
  dry_run: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let repo = Repository::open_from_env()?;
    let config = data::git_config(&repo)?;
    fetch_all(&repo)?;

    // get list of all branches
    let branches = get_all_branches(&repo)?;

    if self.dry_run {
      println!("Deletion candidates:")
    }

    for branch_name in branches {
      // skip base branches
      if cli.config.bases.contains(&branch_name) {
        continue;
      }

      // skip other protected branches
      if cli.config.protect.contains(&branch_name) {
        continue;
      }

      // skip current branch
      if get_current_branch(&repo).is_ok_and(|it| it == branch_name) {
        continue;
      }

      // find base branch from db, or just use the trunk branch
      let base_name =
        data::get_feature_base(&config, &branch_name).unwrap_or(cli.config.trunk.clone());

      // detect if branch is merged (i.e. has no commits that aren't on its base)
      let is_merged = match is_merged(&repo, &branch_name, &base_name) {
        Ok(it) => it,
        Err(e) => {
          eprintln!(
            "Failed to determine if {} is merged into {}: {}",
            branch_name, base_name, e
          );
          continue;
        }
      };
      if is_merged {
        // in dry-run mode, print the branch name but don't delete
        if self.dry_run {
          println!("{}", branch_name);
          continue;
        }

        match repo.find_branch(&branch_name, BranchType::Local) {
          Err(e) => {
            eprintln!("Failed to get reference to branch {}: {}", branch_name, e);
          }
          Ok(mut branch) => {
            if let Err(e) = branch.delete() {
              eprintln!("Failed to delete {}: {}", branch_name, e);
            };
          }
        };
      }
    }

    Ok(())
  }
}
