use git2::{BranchType, Repository};

use crate::cli::{Cli, CliResult, fetch_all, get_all_branches, get_current_branch, is_merged};
use crate::database;

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
    let mut db = database::load(&repo)?;

    if self.dry_run {
      println!("Deletion candidates:")
    }

    for branch in branches {
      // skip base branches
      if cli.config.bases.contains(&branch) {
        continue;
      }

      // skip other protected branches
      if cli.config.protect.contains(&branch) {
        continue;
      }

      // skip current branch
      if get_current_branch(&repo).is_ok_and(|it| it == branch) {
        continue;
      }

      // find base branch from db, or just use the trunk branch
      let base = db.get(&branch).unwrap_or(&cli.config.trunk);

      // detect if branch is merged (i.e. has no commits that aren't on its base)
      if is_merged(&repo, &branch, base).is_ok_and(|yes| yes) {
        // in dry-run mode, print the branch name but don't delete
        if self.dry_run {
          println!("{branch}");
          continue;
        }

        match repo.find_branch(&branch, BranchType::Local) {
          Err(e) => {
            eprintln!("Failed to get reference to branch {branch}: {e}");
          }
          Ok(mut branch_obj) => {
            match branch_obj.delete() {
              Ok(_) => {
                db.remove(&branch);
              }
              Err(e) => {
                eprintln!("Failed to delete {branch}: {e}");
              }
            };
          }
        };
      }
    }

    if !self.dry_run {
      database::save(&repo, db)?;
    }

    Ok(())
  }
}
