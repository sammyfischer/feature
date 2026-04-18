use std::fmt::Write;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{
  Branch,
  ErrorClass,
  ErrorCode,
  FetchOptions,
  Oid,
  PushOptions,
  Remote,
  RemoteCallbacks,
  Repository,
};

use crate::util::branch::get_current_branch_name;
use crate::util::diff::DiffSummary;
use crate::util::display::{display_hash, trim_hash};
use crate::util::get_remote_callbacks;
use crate::{App, lossy};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Pushes a branch to remote, setting upstream automatically")]
pub struct Args {
  /// Force push
  #[arg(short, long)]
  force: bool,

  /// Which remote to push to, if no upstream is already set
  #[arg(short, long)]
  remote: Option<String>,

  /// The name of the upstream branch, if no upstream is already set
  #[arg(short, long)]
  upstream: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let branch_name = get_current_branch_name(&state.repo)?
      .context("Not currently on a branch! Nothing to push.")?;

    // allow pushing bases, but as fast-forward only. the remote can still choose to reject
    if state.config.bases.contains(&branch_name) && self.force {
      return Err(anyhow!("Cannot force push a base branch"));
    }

    // same for protected branches
    if state.config.protect.contains(&branch_name) && self.force {
      return Err(anyhow!("Cannot force push a protected branch"));
    }

    let mut branch = state
      .repo
      .find_branch(&branch_name, git2::BranchType::Local)
      .with_context(|| format!("Failed to get reference to branch {}", branch_name))?;
    let branch_refname = lossy!(&branch.get().name_bytes());

    let (upstream, upstream_name, remote_name) = match branch.upstream() {
      Ok(it) => {
        let name = lossy!(
          &it
            .name_bytes()
            .context("Failed to get existing upstream name")?
        )
        .to_string();

        let (remote_name, upstream_name) = name
          .split_once('/')
          .ok_or(anyhow!("Upstream name has an invalid format"))?;

        (Some(it), upstream_name.to_string(), remote_name.to_string())
      }

      // no upstream set, use flags or defaults
      Err(e) if e.code() == ErrorCode::NotFound => (
        None,
        self.upstream.as_ref().unwrap_or(&branch_name).to_string(),
        self
          .remote
          .as_ref()
          .unwrap_or(&state.config.default_remote)
          .clone(),
      ),

      Err(e) => return Err(e.into()),
    };

    let mut remote = state
      .repo
      .find_remote(&remote_name)
      .with_context(|| format!("Failed to get reference to remote {}", remote_name))?;

    // fetch latest upstream if it exists
    if upstream.is_some() {
      fetch_upstream(&mut remote, &upstream_name, &remote_name)?;
    }

    // get a diff against the latest upstream to print changes at the end
    let summary = get_upstream_diff_summary(&state.repo, &branch, upstream.as_ref())?;

    // TODO: we can do force-with-lease logic by checking the tip of our local branch against the
    // tip of upstream, since we just fetched it.
    // we could also rebase onto upstream, possibly leaving conflicts
    //
    // should also probably check against the base branch

    // prepare actual push
    let mut opts = PushOptions::new();
    opts.remote_callbacks(get_push_callbacks());

    // build the refspec
    let mut refspec = String::with_capacity(40);
    if self.force {
      refspec.push('+');
    }
    write!(refspec, "{}:refs/heads/{}", &branch_refname, &upstream_name)?;

    remote
      .push(&[&refspec], Some(&mut opts))
      .context("Failed to push")?;

    print!(
      "{} {} to {}",
      if self.force {
        style("Force-pushed").yellow()
      } else {
        style("Pushed").green()
      },
      style(&branch_name).blue(),
      style(&remote_name).magenta()
    );

    // set upstream if not already
    if upstream.is_none() {
      let set_upstream_to = format!("{}/{}", &remote_name, &upstream_name);

      print!(
        "{}",
        style(format!(" (tracking {})", set_upstream_to)).dim()
      );

      match branch.set_upstream(Some(&set_upstream_to)) {
        Ok(_) => Ok(()),

        // this error is returned in bare repos where an upstream (e.g. origin/main) cannot be
        // created. in this case, the git config for the branch is still properly set, e.g.
        // `branch.main.remote = origin` and `branch.main.merge = refs/heads/main`
        Err(e) if e.class() == ErrorClass::Reference && e.code() == ErrorCode::NotFound => Ok(()),

        // any other error is a real error
        Err(e) => Err(anyhow!(e).context("Failed to set upstream")),
      }?;
    }

    println!();

    if let Some(summary) = summary.as_ref() {
      println!("New to remote - {}", summary);
    }

    Ok(())
  }
}

/// Fetches and force-updates the latest changes for the given upstream
fn fetch_upstream(remote: &mut Remote, upstream_name: &str, remote_name: &str) -> Result<()> {
  let dst = format!("refs/remotes/{}/{}", remote_name, upstream_name);
  let src = format!("refs/heads/{}", upstream_name);
  let mut opts = FetchOptions::new();
  opts.remote_callbacks(get_remote_callbacks());
  remote.fetch(&[&format!("+{}:{}", src, dst)], Some(&mut opts), None)?;
  Ok(())
}

/// Get diff from upstream to branch. Returns None if upstream is None
fn get_upstream_diff_summary<'repo>(
  repo: &'repo Repository,
  branch: &'repo Branch,
  upstream: Option<&'repo Branch>,
) -> Result<Option<String>> {
  Ok(if let Some(upstream) = upstream {
    let old_tree = upstream.get().peel_to_tree()?;
    let new_tree = branch.get().peel_to_tree()?;

    let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;
    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;

    Some(summary.to_string())
  } else {
    // no upstream exists
    None
  })
}

/// Configures the push callbacks
fn get_push_callbacks<'cbs>() -> RemoteCallbacks<'cbs> {
  let mut cbs = get_remote_callbacks();

  // print error if push fails
  cbs.push_update_reference(|refname, status| {
    // a status of Some means push was rejected
    if let Some(msg) = status {
      eprintln!(
        "{} to {} {}: {}",
        style("Push").red(),
        refname,
        style("failed").red(),
        msg
      );
      return Err(git2::Error::from_str(msg));
    }
    Ok(())
  });

  // this is arbitrary text sent by the server. on github/gitlab, this usually contains info on
  // how to create a pull request for newly pushed branches
  cbs.sideband_progress(|bytes| {
    print!("{}", lossy!(bytes));
    true
  });

  // called on each remote tracking branch that's updated
  cbs.update_tips(|refname, old_id, new_id| {
    let zero = Oid::zero();
    let refname = refname.trim_prefix("refs/remotes/");

    if old_id == new_id {
      return true;
    }

    if old_id == zero {
      println!(
        "{} {} {}",
        style("Created").green(),
        refname,
        display_hash(&new_id)
      );
    } else if new_id == zero {
      println!(
        "{} {} {}",
        style("Deleted").red(),
        refname,
        style(&format!("(was {})", trim_hash(&old_id))).dim()
      );
    } else {
      println!(
        "{} {}: {} -> {}",
        style("Updated").green(),
        refname,
        display_hash(&old_id),
        display_hash(&new_id)
      );
    }
    true
  });

  cbs
}
