use git2::{ErrorCode, PushOptions, Repository};

use crate::cli::{Cli, CliResult, get_current_branch, get_remote_callbacks};
use crate::{cli_err, cli_err_fn};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// Force push
  #[arg(short, long)]
  force: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let repo = Repository::open_from_env()?;
    let branch_name = get_current_branch(&repo)?;

    // allow pushing bases, but as fast-forward only. the remote can still choose to reject
    if cli.config.bases.contains(&branch_name) && self.force {
      return Err(cli_err!(
        Push,
        "This is a base branch, refusing to force push"
      ));
    }

    // same for protected branches
    if cli.config.protect.contains(&branch_name) && self.force {
      return Err(cli_err!(
        Push,
        "This is a protected branch, refusing to force push"
      ));
    }

    let mut branch = repo
      .find_branch(&branch_name, git2::BranchType::Local)
      .map_err(cli_err_fn!(
        Git,
        e,
        "Failed to get reference to {branch_name}: {e}"
      ))?;

    // TODO: consider getting remote name from upstream if it exists, then default to this
    let remote_name = &cli.config.default_remote;
    let mut remote = repo.find_remote(remote_name).map_err(cli_err_fn!(
      Git,
      e,
      "Failed to get reference to default remote: {e}"
    ))?;

    // if there's already an upstream, use that. else use current branch name and set upstream at
    // the end
    let mut has_upstream = false;
    let upstream_name = match branch.upstream() {
      Ok(it) => {
        let name = it
          .name()?
          .ok_or(cli_err!(Git, "Upstream branch name is not valid utf-8"))?;

        let name = name
          // remote the origin/ prefix, as we don't want it in the refspec
          .strip_prefix(&format!("{}/", remote_name))
          .ok_or(cli_err!(
            Push,
            "Detected upstream {}, but it doesn't belong to the default remote: {}",
            name,
            remote_name
          ))?
          .to_string();

        has_upstream = true;
        name
      }

      // upstream not found, create it with the same name as branch
      Err(e) if e.code() == ErrorCode::NotFound => branch_name.clone(),
      Err(e) => {
        return Err(cli_err!(Git, "Failure when getting upstream: {e}"));
      }
    };

    let mut opts = PushOptions::new();
    let mut cbs = get_remote_callbacks();

    // print error if push fails
    cbs.push_update_reference(|refname, status| {
      // a status of Some means push was rejected
      if let Some(msg) = status {
        eprintln!("Push to {} was rejected: {}", refname, msg);
        return Err(git2::Error::from_str(msg));
      }
      Ok(())
    });

    opts.remote_callbacks(cbs);

    // Some info on refspecs (from https://git-scm.com/book/en/v2/Git-Internals-The-Refspec)
    //
    // Full syntax: `+<src>:<dst>` where the '+' is optional
    //
    // For fetches/pulls, src will be a ref on remote, and for pushes it's a local ref. e.g. `fetch
    // +refs/heads/main:refs/remotes/origin/main` gets refs/heads/main from remote, and puts it on a
    // local copy called refs/remotes/origin/main. And a pull performs a subsequent merge or
    // refs/remotes/origin/main into your local refs/heads/main.
    //
    // For push, you most likely want both sides to start with refs/heads, since you're pushing a
    // local working copy to the remote working copy. Branches in refs/remotes exist only as a cache
    // for the actual remote branch, and are useful as backups.
    //
    // "The + tells Git to update the reference even if it isn’t a fast-forward."
    // Exclusion of a '+' is convenient for fast-forward only, e.g. when working with base branches.
    // Inclusion of a '+' can be used to force push.
    //
    // Git expands refspecs in an intuitive way. If the refspec is `main:main`, git will expand this
    // to `refs/heads/main:refs/heads/main` for a push.

    // build the refspec
    let mut refspec = String::new();
    if self.force {
      refspec.push('+');
    }
    refspec.push_str(&branch_name);
    refspec.push(':');
    refspec.push_str("refs/heads/");
    refspec.push_str(&branch_name);

    remote
      .push(&[&refspec], Some(&mut opts))
      .map_err(cli_err_fn!(Git, e, "Failed to push: {e}"))?;

    // set upstream if not already
    if !has_upstream {
      let set_upstream_to = format!("{}/{}", remote_name, upstream_name);
      println!("Setting upstream to: {}", set_upstream_to);

      branch
        .set_upstream(Some(&set_upstream_to))
        .map_err(cli_err_fn!(
          Git,
          e,
          "Failed to set upstream tracking branch: {e}"
        ))?;
    }

    Ok(())
  }
}
