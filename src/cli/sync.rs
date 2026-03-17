use git2::Repository;

use crate::cli::error::CliError;
use crate::cli::{
  Cli,
  CliResult,
  can_fast_forward,
  fetch_all,
  get_current_branch,
  has_local_changes,
};
use crate::{await_child, database, git};

pub fn sync(cli: &Cli) -> CliResult {
  let repo = Repository::open_from_env()?;
  fetch_all(&repo)?;

  if has_local_changes(&repo)? {
    return Err(CliError::Generic(
      "You have uncommitted changes! Please commit or stash them before syncing".into(),
    ));
  }

  // save current branch to switch back to at the end
  let start_branch = get_current_branch(&repo)?;

  let bases = &cli.config.bases;

  // whether the script switched to a different branch
  let mut has_switched = false;

  // error messages to print at the end
  let mut failures: Vec<String> = Vec::new();

  for branch in bases {
    // switch to branch
    let Ok(mut child) = git!("switch", branch).spawn() else {
      failures.push(format!("Failed to switch to branch: {}", branch));
      continue;
    };
    let Ok(_) = await_child!(child, format!("Failed to switch to branch: {}", branch)) else {
      failures.push(format!("Failed to switch to branch: {}", branch));
      continue;
    };

    has_switched = true;

    if let Ok(yes) = can_fast_forward(&repo, branch) {
      if !yes {
        failures.push(format!(
          "Cannot fast forward branch: {}. You might want to pull manually",
          branch
        ));
      }
    } else {
      failures.push(format!("Failed to check if {} is fast-forwardable", branch));
      continue;
    }

    // pull changes (fast-forward only)
    let Ok(mut child) = git!("pull", "--ff-only").spawn() else {
      failures.push(format!("Failed to pull changes into branch: {}", branch));
      continue;
    };
    let Ok(_) = await_child!(
      child,
      format!("Failed to pull changes into branch: {}", branch)
    ) else {
      failures.push(format!("Failed to pull changes into branch: {}", branch));
      continue;
    };
  }

  if has_switched {
    // switch back
    if let Ok(mut child) = git!("switch", &start_branch).spawn()
      && await_child!(child, "Failed to switch back to starting branch").is_err()
    {
      failures.push(format!(
        "Failed to switch back to starting branch: {}",
        &start_branch
      ));
    };
  }

  // clean deleted branches from db
  if let Ok(mut db) = database::load(&repo) {
    database::clean(&mut db);

    if let Err(e) = database::save(&repo, db) {
      failures.push(format!("Failed to save database changes: {}", e));
    };
  } else {
    failures.push("Failed to load database".into());
  }

  if !failures.is_empty() {
    eprintln!("{}", failures.join("\n"));
  }

  Ok(())
}
