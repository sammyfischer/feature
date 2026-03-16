use git2::Repository;

use crate::cli::{Cli, CliResult, fetch_all, get_all_branches, get_current_branch, is_merged};
use crate::{await_child, database, git};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  #[arg(long)]
  dry_run: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let repo = Repository::open(".")?;
    fetch_all(&repo)?;

    // get list of all branches
    let branches = get_all_branches(&repo)?;
    let mut db = database::load()?;

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
      if is_merged(&branch, base).is_ok_and(|yes| yes) {
        // in dry-run mode, print the branch name but don't delete
        if self.dry_run {
          println!("{}", &branch);
          continue;
        }

        // delete 1 by 1 (use -D to force delete, we've assured all commits exist on the base)
        if let Ok(mut child) = git!("branch", "-D", &branch).spawn() {
          if await_child!(child, format!("Failed to delete branch {}", &branch)).is_err() {
            eprintln!("Failed to delete branch {}", &branch);
          } else {
            // remove deleted branch from db
            db.remove(&branch);
          }
        } else {
          eprintln!("Failed to delete branch {}", &branch);
        };
      }
    }

    Ok(())
  }
}
