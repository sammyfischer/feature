use std::fmt::Write;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{
  BranchType,
  ErrorClass,
  ErrorCode,
  FetchOptions,
  ObjectType,
  Oid,
  PushOptions,
  Reference,
  Remote,
  RemoteCallbacks,
  Repository,
  ResetType,
};

use crate::util::branch::get_ahead_behind;
use crate::util::diff::DiffSummary;
use crate::util::display::{display_hash, trim_hash};
use crate::util::get_remote_callbacks;
use crate::{App, data, lossy, style};

const UPSTREAM_DIVERGED_MSG: &str = r"Branch has diverged from its upstream. You must:

1. Resolve the differences, for example:
   • git pull [--merge | --rebase]
   • git cherry-pick (each new commit on upstream)
2. Push again. You'll most likely need to force push if you've done any
   cherry-picks or rebases.";

const BASE_DIVERGED_MSG: &str = r"Branch has diverged from its base. You must:

1. Resolve the differences, for example:
   • git rebase/merge <base>
   • feature update
2. Push again. You'll most likely need to force push if you've done a feature
   update or git rebase.";

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
    let head = state.repo.find_reference("HEAD")?;
    let branch_refname = lossy!(
      head
        .symbolic_target_bytes()
        .context("Not checked out to a branch, nothing to push!")?
    )
    .to_string();
    let branch_ref = state.repo.find_reference(&branch_refname)?;

    // make sure it's a branch, it could be a tag
    if !branch_ref.is_branch() {
      return Err(anyhow!("Not checked out to a branch, nothing to push!"));
    }
    let branch_name = lossy!(branch_ref.shorthand_bytes()).to_string();

    // allow pushing protected branches, but as fast-forward only
    if state.config.protect.contains(&branch_name) && self.force {
      return Err(anyhow!("Cannot force push a protected branch"));
    }

    let upstream_refname = match state.repo.branch_upstream_name(&branch_refname) {
      Ok(it) => Some(lossy!(&it).to_string()),
      Err(e) if e.code() == ErrorCode::NotFound => None,
      Err(e) => return Err(e.into()),
    };

    let remote_name = match &upstream_refname {
      Some(upstream_refname) => {
        lossy!(&state.repo.branch_remote_name(upstream_refname)?).to_string()
      }
      None => self
        .remote
        .as_ref()
        .unwrap_or(&state.config.default_remote)
        .clone(),
    };

    let mut remote = state
      .repo
      .find_remote(&remote_name)
      .with_context(|| format!("Failed to get reference to remote {}", remote_name))?;

    // fetches the latest upstream, checks if new changes can be resolved
    self.check_upstream(
      &state.repo,
      &branch_refname,
      upstream_refname.as_deref(),
      &mut remote,
    )?;

    // fetches the latest base, checks if new changes can be resolved
    self.check_base(&state.repo, &branch_refname)?;

    // get the changes that were pushed to remote to print later
    let summary = if let Some(upstream_refname) = &upstream_refname {
      // get the branch again, in case the fetch changed the reference
      let upstream_ref = state.repo.find_reference(upstream_refname)?;
      let old_tree = upstream_ref.peel_to_tree()?;

      let branch_ref = state.repo.find_reference(&branch_refname)?;
      let new_tree = branch_ref.peel_to_tree()?;

      let mut diff = state
        .repo
        .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;
      diff.find_similar(None)?;

      let summary = DiffSummary::new(&diff)?;
      Some(summary.to_string())
    } else {
      None
    };

    // prepare actual push
    let mut opts = PushOptions::new();
    opts.remote_callbacks(get_push_callbacks());

    // build the refspec
    let mut refspec = String::with_capacity(40);
    if self.force {
      refspec.push('+');
    }

    let upstream_name = match &upstream_refname {
      // use existing upstream (shorthand) name if available
      Some(it) => lossy!(state.repo.find_reference(it)?.shorthand_bytes()).to_string(),

      // use arg passed by user, defaulting to the same name as the branch
      None => format!(
        "{}/{}",
        remote_name,
        self.upstream.as_ref().unwrap_or(&branch_name)
      ),
    };
    // the destination should be as it appears on remote, which is why it starts with refs/heads/
    // instead of refs/remotes/
    //
    // upstream_name is of the form remote/branch
    write!(
      refspec,
      "{}:refs/heads/{}",
      &branch_refname,
      &upstream_name
        .split_once('/')
        .expect("Invalid format for upstream branch name")
        .1
    )?;

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
    if upstream_refname.is_none() {
      let mut branch = state.repo.find_branch(&branch_name, BranchType::Local)?;
      match branch.set_upstream(Some(&upstream_name)) {
        Ok(_) => Ok(()),

        // this error is returned in bare repos where an upstream (e.g. origin/main) cannot be
        // created. in this case, the git config for the branch is still properly set, e.g.
        // `branch.main.remote = origin` and `branch.main.merge = refs/heads/main`
        Err(e) if e.class() == ErrorClass::Reference && e.code() == ErrorCode::NotFound => Ok(()),

        // any other error is a real error
        Err(e) => Err(anyhow!(e).context("Failed to set upstream")),
      }?;

      print!("{}", style(format!(" (tracking {})", &upstream_name)).dim());
    }

    println!();

    if let Some(summary) = summary.as_ref() {
      println!("New to remote - {}", summary);
    }

    Ok(())
  }

  /// Fetches the latest upstream ensures that we have all the needed changes
  fn check_upstream(
    &self,
    repo: &Repository,
    branch_refname: &str,
    upstream_refname: Option<&str>,
    remote: &mut Remote,
  ) -> Result<()> {
    if let Some(upstream_refname) = upstream_refname {
      let upstream_ref = repo.find_reference(upstream_refname)?;
      let upstream_name = lossy!(upstream_ref.shorthand_bytes());
      let upstream_super_short_name = upstream_name
        .split_once('/')
        .expect("Invalid format for upstream branch name")
        .1;

      let refspec = format!(
        "+refs/heads/{}:{}",
        upstream_super_short_name, upstream_refname
      );

      let mut opts = FetchOptions::new();
      opts.remote_callbacks(get_remote_callbacks());
      remote.fetch(&[&refspec], Some(&mut opts), None)?;

      println!("{}", style!("Fetched {}", upstream_name).dim());

      if !self.force {
        // get the new reference after the fetch
        let upstream_ref = repo.find_reference(upstream_refname)?;

        let branch_ref = repo.find_reference(branch_refname)?;
        let branch_name = lossy!(branch_ref.shorthand_bytes());

        let ab = repo.graph_ahead_behind(
          branch_ref.peel_to_commit()?.id(),
          upstream_ref.peel_to_commit()?.id(),
        )?;

        match ab {
          // up to date, continue to check against base
          (a, b) if a == 0 && b == 0 => {}
          // local is ahead, continue with push (and check against base)
          (a, b) if a > 0 && b == 0 => {}

          // local is behind, fast forward (soft reset)
          (a, b) if a == 0 && b > 0 => {
            soft_reset(repo, &upstream_ref)?;
            println!(
              "{}",
              style!("Fast-forwarded {} to {}", branch_name, &upstream_name).dim()
            );
          }

          // divergent histories, user must resolve
          (a, b) if a > 0 && b > 0 => {
            eprintln!("{}", UPSTREAM_DIVERGED_MSG);
            return Err(anyhow!("Diverged from upstream"));
          }

          (a, b) => {
            return Err(anyhow!(
              "Unexpected ahead/behind against upstream: ahead {}, behind {}",
              a,
              b
            ));
          }
        }
      }
    }

    Ok(())
  }

  fn check_base(&self, repo: &Repository, branch_refname: &str) -> Result<()> {
    let branch_ref = repo.find_reference(branch_refname)?;
    let branch_name = lossy!(branch_ref.shorthand_bytes());

    // if base exists
    if let Some(base_refname) = data::get_feature_base(&data::git_config(repo)?, &branch_name) {
      // if it's a remote, fetch latest changes
      if base_refname.starts_with("refs/remotes") {
        let base_ref = repo.find_reference(&base_refname)?;
        let base_name = lossy!(base_ref.shorthand_bytes());
        let base_super_short_name = base_name
          .split_once('/')
          .expect("Invalid format for base branch name")
          .1;

        let remote_name = repo.branch_remote_name(&base_refname)?;
        let remote_name = lossy!(&remote_name);

        let mut remote = repo.find_remote(&remote_name)?;
        let refspec = format!("+refs/heads/{}:{}", base_super_short_name, base_refname);

        let mut opts = FetchOptions::new();
        opts.remote_callbacks(get_remote_callbacks());
        remote.fetch(&[&refspec], Some(&mut opts), None)?;

        println!("{}", style!("Fetched {}", base_name).dim());
      }

      if !self.force {
        let base_ref = repo.find_reference(&base_refname)?;
        let base_name = lossy!(base_ref.shorthand_bytes());

        let ab = get_ahead_behind(repo, &branch_ref, &base_ref)?;

        match ab {
          // already up to date, continue with push
          (a, b) if a == 0 && b == 0 => {}

          // branch is ahead, continue with push
          (a, b) if a > 0 && b == 0 => {}

          // branch is behind, need those changes
          (a, b) if a == 0 && b > 0 => {
            soft_reset(repo, &base_ref)?;
            println!(
              "{}",
              style!("Fast-forwarded {} to {}", branch_name, &base_name).dim()
            );
          }

          // divergent histories, user must resolve
          (a, b) if a > 0 && b > 0 => {
            eprintln!("{}", BASE_DIVERGED_MSG);
            return Err(anyhow!("Diverged from base"));
          }

          (a, b) => {
            return Err(anyhow!(
              "Unexpected ahead/behind against upstream: ahead {}, behind {}",
              a,
              b
            ));
          }
        }
      }
    };

    Ok(())
  }
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

/// Reset current branch and HEAD to branch_ref
fn soft_reset(repo: &Repository, branch_ref: &Reference) -> Result<()> {
  let obj = repo.find_object(branch_ref.peel_to_commit()?.id(), Some(ObjectType::Commit))?;
  repo.reset(&obj, ResetType::Soft, None)?;
  Ok(())
}
