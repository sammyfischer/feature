use anyhow::{Context, Result};
use console::style;
use git2::FetchOptions;
use regex::Regex;

use crate::cli::{Cli, get_current_branch, get_remote_callbacks};
use crate::open_repo;

pub const LONG_ABOUT: &str = r"Updates all base branches with their remotes.

Fast-forwards local branches (e.g. refs/heads/*) and force-updates remotes
(e.g. refs/remotes/origin/*).";

pub fn run(cli: &Cli) -> Result<()> {
  let repo = open_repo!();

  let start_branch = get_current_branch(&repo).context("Failed to get current branch")?;
  let bases = &cli.config.bases;

  // matches remote/branch, captures the remote name and branch name on remote
  // the remote is not allowed to contain slashes, so that it matches up to the first slash
  let re = Regex::new(r"([^\s/]+)/(\S+)").expect("Failed to compile a regex pattern");

  let mut opts = FetchOptions::new();
  opts.remote_callbacks(get_remote_callbacks());

  for branch_name in bases {
    let out: String;

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

          out = format!(
            "{} {} {}",
            style("Updated").green(),
            branch_name,
            style(format!("(and {}/{})", remote_name, upstream_name)).dim()
          );

          (
            format!("refs/heads/{}:refs/heads/{}", branch_name, upstream_name),
            format!(
              "+refs/heads/{}:refs/remotes/{}/{}",
              branch_name, remote_name, upstream_name
            ),
            captures[1].to_string(),
          )
        }
        None => {
          out = format!(
            "{} {} {}",
            style("Updated").green(),
            branch_name,
            style(format!(
              "(and {}/{})",
              cli.config.default_remote, branch_name
            ))
            .dim()
          );

          (
            format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name),
            format!(
              "+refs/heads/{}:refs/remotes/{}/{}",
              branch_name, cli.config.default_remote, branch_name
            ),
            cli.config.default_remote.clone(),
          )
        }
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

    println!("{}", out);
  }

  Ok(())
}
