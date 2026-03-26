use git2::{FetchOptions, Repository};

use crate::cli::error::CliError;
use crate::cli::{
  Cli,
  CliResult,
  fetch_all,
  get_current_branch,
  get_remote_callbacks,
  has_local_changes,
};
use crate::{cli_err_fn, database};

pub fn sync(cli: &Cli) -> CliResult {
  let repo = Repository::open_from_env()?;
  fetch_all(&repo)?;

  if has_local_changes(&repo)? {
    return Err(CliError::Generic(
      "You have uncommitted changes! Please commit or stash them before syncing".into(),
    ));
  }

  let start_branch = get_current_branch(&repo)?;
  let bases = &cli.config.bases;

  // matches remote/branch, captures the remote name and branch name on remote
  // the remote is not allowed to contain slashes, so that it matches up to the first slash
  let re = regex::Regex::new(r"([^\s/]+)/(\S+)").map_err(cli_err_fn!(
    Generic,
    e,
    "Failed to compile a regex pattern: {e}"
  ))?;

  let mut opts = FetchOptions::new();
  opts.remote_callbacks(get_remote_callbacks());

  for branch_name in bases {
    if branch_name == &start_branch {
      println!("Already checked out to {}. Skipping over it", branch_name);
      continue;
    }

    let Ok(branch) = repo.find_branch(branch_name, git2::BranchType::Local) else {
      eprintln!("Failed to get info for branch: {}", branch_name);
      continue;
    };

    let Ok(upstream) = branch.upstream() else {
      eprintln!("Failed to find upstream of {}", branch_name);
      continue;
    };

    let Ok(Some(upstream_long_name)) = upstream.name() else {
      eprintln!("Failed to get upstream name of {}", branch_name);
      continue;
    };

    // head_refspec is the actual local branch, located in refs/heads
    // remote_refspec is the local copy of remote, located in refs/remotes/remote_name (and always
    // force-updates)
    let (head_refspec, remote_refspec, remote_name): (String, String, String) =
      match re.captures(upstream_long_name) {
        Some(captures) => {
          let remote_name = &captures[1];
          let upstream_name = &captures[2];
          (
            format!("refs/heads/{}:refs/heads/{}", branch_name, upstream_name),
            format!(
              "+refs/heads/{}:refs/remotes/{}/{}",
              branch_name, remote_name, upstream_name
            ),
            captures[1].to_string(),
          )
        }
        None => (
          format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name),
          format!(
            "+refs/heads/{}:refs/remotes/{}/{}",
            branch_name, cli.config.default_remote, branch_name
          ),
          cli.config.default_remote.clone(),
        ),
      };

    let mut remote = match repo.find_remote(&remote_name) {
      Ok(it) => it,
      Err(e) => {
        eprintln!("Failed to find remote {}: {}", remote_name, e);
        continue;
      }
    };

    // TODO: threadpool these?
    if let Err(e) = remote.fetch(&[&head_refspec, &remote_refspec], Some(&mut opts), None) {
      eprintln!("Failed to fetch {}: {}", branch_name, e);
    };
  }

  // clean deleted branches from db
  if let Ok(mut db) = database::load(&repo) {
    database::clean(&mut db);

    if let Err(e) = database::save(&repo, db) {
      eprintln!("Failed to save database changes: {}", e);
    };
  } else {
    eprintln!("Failed to load database");
  }

  Ok(())
}
