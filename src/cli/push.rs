use std::fmt::Write;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{
  Branch,
  BranchType,
  ErrorClass,
  ErrorCode,
  FetchOptions,
  PushOptions,
  RemoteCallbacks,
  Repository,
};

use crate::util::branch::{get_ahead_behind, soft_reset};
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::DiffSummary;
use crate::util::lossy::ToStrLossy;
use crate::util::{credentials_cb, get_update_tips_cb};
use crate::{App, data, style};

const NO_BRANCH_MSG: &str = r#"You must be checked out to a branch or specify one manually as the last
argument, e.g. "feature push my-branch".""#;

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

  /// The branch to push. Defaults to current branch
  #[arg(value_name = "BRANCH-ISH")]
  branch: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let branch = match &self.branch {
      Some(branch_name) => BranchMeta::from_name_dwim(&state.repo, branch_name)?
        .ok_or(anyhow!("Branch not found: {}", branch_name))?,
      None => BranchMeta::current(&state.repo)?.context(NO_BRANCH_MSG)?,
    };

    // allow pushing protected branches, but as fast-forward only
    if state.config.protect.iter().any(|it| it == branch.name()) && self.force {
      return Err(anyhow!("Cannot force push a protected branch"));
    }

    let (upstream, remote_name) = match branch.upstream(&state.repo)? {
      Some(it) => {
        let meta = BranchMeta::from_branch(&it)?;
        let remote_name = meta
          .split_name_and_remote()?
          .1
          .expect("Upstream should have a remote");
        (Some(meta), remote_name)
      }
      None => (
        None,
        self
          .remote
          .as_ref()
          .unwrap_or(&state.config.default_remote)
          .clone(),
      ),
    };

    // fetches the latest upstream, checks if new changes can be resolved
    match check_upstream(&state.repo, &branch, upstream.as_ref(), self.force)? {
      // do nothing, no upstream to check
      PushCheckStatus::NoBranch => {}

      // do nothing, the user wants to push
      PushCheckStatus::Forced => {}

      // do nothing, push is possible
      PushCheckStatus::UpToDate => {}

      // local is ahead, safe to push
      PushCheckStatus::Ahead => {}

      // fast-forward the branch
      PushCheckStatus::Behind => {
        if let Some(upstream) = upstream.as_ref() {
          soft_reset(&state.repo, &upstream.resolve(&state.repo)?)?;
          println!(
            "{}",
            style!("Fast-forwarded {} to {}", branch.name(), upstream.name()).dim()
          );
        }
      }

      PushCheckStatus::Diverged => return Err(anyhow!(UPSTREAM_DIVERGED_MSG)),
    }

    // fetches the latest base, checks if new changes can be resolved
    let base = data::get_feature_base(&state.repo, branch.name())?;
    match check_base(&state.repo, &branch, base.as_ref(), self.force)? {
      PushCheckStatus::NoBranch => {}
      PushCheckStatus::Forced => {}
      PushCheckStatus::UpToDate => {}
      PushCheckStatus::Ahead => {}
      PushCheckStatus::Behind => {
        if let Some(base) = base {
          soft_reset(&state.repo, &base.resolve(&state.repo)?)?;
          println!(
            "{}",
            style!("Fast-forwarded {} to {}", branch.name(), base.name()).dim()
          );
        }
      }
      PushCheckStatus::Diverged => return Err(anyhow!(BASE_DIVERGED_MSG)),
    };

    // get the changes that were pushed to remote to print later
    let summary = if let Some(upstream) = upstream.as_ref() {
      // get the branch again, in case the fetch changed the reference
      let upstream_ref = upstream.resolve(&state.repo)?;
      let old_tree = upstream_ref.peel_to_tree()?;

      let branch_ref = state.repo.find_reference(branch.refname())?;
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
    opts.remote_callbacks(get_push_callbacks(&state.repo));

    // build the refspec
    let mut refspec = String::with_capacity(40);
    if self.force {
      refspec.push('+');
    }

    let upstream_name = match upstream.as_ref() {
      // use existing upstream (shorthand) name if available
      Some(it) => it.name().to_string(),

      // use arg passed by user, defaulting to the same name as the branch
      None => format!(
        "{}/{}",
        remote_name,
        self.upstream.as_deref().unwrap_or(branch.name())
      ),
    };

    // the destination should be as it appears on remote, which is why it starts with refs/heads/
    // instead of refs/remotes/
    //
    // upstream_name is of the form remote/branch
    write!(
      refspec,
      "{}:refs/heads/{}",
      branch.refname(),
      &upstream_name
        .split_once('/')
        .expect("Invalid format for upstream branch name")
        .1
    )?;

    let mut remote = state
      .repo
      .find_remote(&remote_name)
      .with_context(|| format!("Failed to get reference to remote {}", remote_name))?;

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
      style(branch.name()).blue(),
      style(&remote_name).magenta()
    );

    // set upstream if not already
    if upstream.is_none() {
      let mut branch = Branch::wrap(branch.resolve(&state.repo)?);
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
}

pub enum PushCheckStatus {
  /// The branch being checked against doesn't exist
  NoBranch,

  /// Ahead/behind checks were not performed, but the branch exists
  Forced,

  /// Both branches point to the same commit
  UpToDate,

  /// Ahead of the branch being checked against
  Ahead,

  /// Behind the branch being checked against
  Behind,

  /// Branches have diverged
  Diverged,
}

/// Fetches the latest upstream ensures that we have all the needed changes
pub fn check_upstream(
  repo: &Repository,
  branch: &BranchMeta,
  upstream: Option<&BranchMeta>,
  force: bool,
) -> Result<PushCheckStatus> {
  let Some(upstream) = upstream else {
    return Ok(PushCheckStatus::NoBranch);
  };

  if !upstream.is_remote() {
    return Err(anyhow!(
      "Upstream is not a remote branch: {}",
      upstream.name()
    ));
  }

  let (_, remote_name) = upstream.split_name_and_remote()?;
  let mut remote = repo.find_remote(&remote_name.unwrap_or_else(|| {
    panic!(
      "Remote should exist on upstream branch: {}",
      upstream.name()
    )
  }))?;

  let refspec = format!(
    "+refs/heads/{}:{}",
    upstream.split_name_and_remote()?.0,
    upstream.refname()
  );

  let mut opts = FetchOptions::new();
  let mut cbs = RemoteCallbacks::new();
  cbs.credentials(credentials_cb);
  opts.remote_callbacks(cbs);

  remote.fetch(&[&refspec], Some(&mut opts), None)?;

  println!("{}", style!("Fetched {}", upstream.name()).dim());

  if force {
    return Ok(PushCheckStatus::Forced);
  }

  let branch_tip = branch.resolve(repo)?.peel_to_commit()?;
  let upstream_tip = upstream.resolve(repo)?.peel_to_commit()?;

  // get the new reference after the fetch
  let ab = repo.graph_ahead_behind(branch_tip.id(), upstream_tip.id())?;

  Ok(match ab {
    // up to date, continue to check against base
    (a, b) if a == 0 && b == 0 => PushCheckStatus::UpToDate,

    // local is ahead, continue with push (and check against base)
    (a, b) if a > 0 && b == 0 => PushCheckStatus::Ahead,

    // local is behind, fast forward (soft reset)
    (a, b) if a == 0 && b > 0 => PushCheckStatus::Behind,

    // divergent histories, user must resolve
    (a, b) if a > 0 && b > 0 => PushCheckStatus::Diverged,

    (a, b) => {
      return Err(anyhow!(
        "Unexpected ahead/behind against upstream: ahead {}, behind {}",
        a,
        b
      ));
    }
  })
}

/// Fetches the latest base ensures that we have all the needed changes
pub fn check_base(
  repo: &Repository,
  branch: &BranchMeta,
  base: Option<&BranchMeta>,
  force: bool,
) -> Result<PushCheckStatus> {
  let Some(base) = base else {
    return Ok(PushCheckStatus::NoBranch);
  };

  if base.ty() == BranchType::Remote {
    let (shorter_name, remote_name) = base.split_name_and_remote()?;
    let mut remote = repo.find_remote(
      &remote_name
        .unwrap_or_else(|| panic!("Remote should exist on upstream branch: {}", base.name())),
    )?;

    let refspec = format!("+refs/heads/{}:{}", shorter_name, base.refname());

    let mut opts = FetchOptions::new();
    let mut cbs = RemoteCallbacks::new();
    cbs.credentials(credentials_cb);
    opts.remote_callbacks(cbs);

    remote.fetch(&[&refspec], Some(&mut opts), None)?;

    println!("{}", style!("Fetched {}", base.name()).dim());
  }

  if force {
    return Ok(PushCheckStatus::Forced);
  }

  let base_ref = base.resolve(repo)?;
  let ab = get_ahead_behind(repo, &branch.resolve(repo)?, &base_ref)?;

  Ok(match ab {
    // already up to date, continue with push
    (a, b) if a == 0 && b == 0 => PushCheckStatus::UpToDate,

    // branch is ahead, continue with push
    (a, b) if a > 0 && b == 0 => PushCheckStatus::Ahead,

    // branch is behind, need those changes
    (a, b) if a == 0 && b > 0 => PushCheckStatus::Behind,

    // divergent histories, user must resolve
    (a, b) if a > 0 && b > 0 => PushCheckStatus::Diverged,

    (a, b) => {
      return Err(anyhow!(
        "Unexpected ahead/behind against upstream: ahead {}, behind {}",
        a,
        b
      ));
    }
  })
}

/// Configures the push callbacks
fn get_push_callbacks<'cbs>(repo: &'cbs Repository) -> RemoteCallbacks<'cbs> {
  let mut cbs = RemoteCallbacks::new();

  cbs.credentials(credentials_cb);

  // called on each remote tracking branch that's updated
  cbs.update_tips(get_update_tips_cb(repo));

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
    print!("{}", bytes.to_str_lossy());
    true
  });

  cbs
}
