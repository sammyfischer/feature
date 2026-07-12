use std::fmt::Write;

use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use console::style;
use git2::{Branch, ErrorClass, ErrorCode, PushOptions, Repository};

use crate::cli::display::diff::display_summary;
use crate::cli::display::display_hash;
use crate::core::branch_info::BranchInfo;
use crate::core::diff::DiffSummary;
use crate::core::push::check::{PushCheckStatus, check_base, check_upstream};
use crate::core::push::{
  PushRejection,
  PushStatus,
  PushUpdate,
  PushUpdateKind,
  get_push_callbacks,
};
use crate::core::string::TrimPrefix;
use crate::core::trim_hash;
use crate::core::user_config::UserConfig;
use crate::{App, style};

const NO_BRANCH_MSG: &str = r#"You must be checked out to a branch or specify one manually as the last
argument, e.g. "feature push my-branch".""#;

const UPSTREAM_DIVERGED_MSG: &str = r"Branch has diverged from its upstream. You must:

1. Resolve the differences, for example:
   • git pull [--merge | --rebase]
2. Push again. You'll most likely need to force push if you've done any
   cherry-picks or rebases.";

const BASE_DIVERGED_MSG: &str = r"Branch has diverged from its base. You must:

1. Resolve the differences, for example:
   • git rebase/merge <base>
   • feature update
2. Push again. You'll most likely need to force push if you've done a feature
   update or git rebase.";

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Pushes a branch to remote, setting upstream automatically",
  disable_help_subcommand = true
)]
pub struct Args {
  /// Force push
  #[arg(short, long)]
  force: bool,

  /// Which remote to push to, if no upstream is already set
  #[arg(short, long, value_hint = ValueHint::Other)]
  remote: Option<String>,

  /// The name of the upstream branch, if no upstream is already set
  #[arg(short, long, value_name = "BRANCH", value_hint = ValueHint::Other)]
  upstream: Option<String>,

  /// The branch to push. Defaults to current branch
  #[arg(value_name = "BRANCH", value_hint = ValueHint::Other)]
  branch: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let branch = match &self.branch {
      Some(branch_name) => BranchInfo::from_name_dwim(&state.repo, branch_name)?
        .ok_or(anyhow!("Branch not found: {}", branch_name))?,
      None => BranchInfo::current(&state.repo)?.context(NO_BRANCH_MSG)?,
    };

    // allow pushing protected branches, but as fast-forward only
    if state.config.protect.iter().any(|it| it == branch.name()) && self.force {
      return Err(anyhow!("Cannot force push a protected branch"));
    }

    let (upstream, remote_name) = match branch.upstream(&state.repo)? {
      Some(it) => {
        let info = BranchInfo::from_branch(&it)?;
        let remote_name = info
          .split_name_and_remote()?
          .1
          .expect("Upstream should have a remote");
        (Some(info), remote_name)
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
      // continue, no upstream to check
      PushCheckStatus::NoBranch => {}

      // push no matter what
      PushCheckStatus::Forced => {}

      // nothing to push
      PushCheckStatus::UpToDate => {
        println!("Already up to date, nothing to push");
        return Ok(());
      }

      // safe to push
      PushCheckStatus::Ahead => {}

      // nothing to push
      PushCheckStatus::Behind => {
        println!("Branch is behind remote, nothing new to push");
        return Ok(());
      }

      // unsafe to push
      PushCheckStatus::Diverged => return Err(anyhow!(UPSTREAM_DIVERGED_MSG)),
    }

    let user_config = UserConfig::new(&state.repo)?;

    // fetches the latest base, checks if new changes can be resolved
    let base = user_config.branch_base(branch.name())?;
    match check_base(&state.repo, &branch, base.as_ref(), self.force)? {
      PushCheckStatus::NoBranch => {}
      PushCheckStatus::Forced => {}
      PushCheckStatus::UpToDate => {}
      PushCheckStatus::Ahead => {}
      PushCheckStatus::Behind => {}
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
      Some(display_summary(&summary))
    } else {
      None
    };

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

    // the destination should be as it appears on remote, which is why it starts
    // with refs/heads/ instead of refs/remotes/
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

    // perform push and display output
    let mut status = PushStatus::new();
    {
      let mut opts = PushOptions::new();
      opts.remote_callbacks(get_push_callbacks(&mut status)?);

      remote
        .push(&[&refspec], Some(&mut opts))
        .context("Failed to push")?;

      // drop opts after push
    }

    println!("{}", display_push_status(&state.repo, status)?);

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
        Ok(_) => {
          print!("{}", style(format!(" (tracking {})", &upstream_name)).dim());
          Ok(())
        }

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

/// Display push results
pub fn display_push_status(repo: &Repository, output: PushStatus) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::new();
  let mut first = true;

  let (updates, rejections, response) = output.into_inner();

  for update in updates {
    if first {
      first = false;
    } else {
      writeln!(out)?;
    }
    write!(out, "{}", &display_push_update(repo, &update)?)?;
  }

  for rejection in rejections {
    if first {
      first = false;
    } else {
      writeln!(out)?;
    }
    write!(out, "{}", &display_push_rejection(&rejection))?;
  }

  if !first {
    writeln!(out)?;
  }
  write!(out, "{}", response.trim())?;

  Ok(out)
}

fn display_push_update(repo: &Repository, update: &PushUpdate) -> Result<String> {
  let name = update.refname.trim_prefix_opt("refs/remotes/");
  match update.kind {
    PushUpdateKind::Create(id) => {
      let commit = repo.find_commit(id)?;

      Ok(format!(
        "{} {}: {}",
        style("Created").green(),
        name,
        display_hash(commit.as_object())?
      ))
    }

    PushUpdateKind::Update(old_id, new_id) => {
      let old_commit = repo.find_commit(old_id)?;
      let new_commit = repo.find_commit(new_id)?;

      Ok(format!(
        "{} {}: {} -> {}",
        style("Updated").green(),
        name,
        display_hash(old_commit.as_object())?,
        display_hash(new_commit.as_object())?
      ))
    }

    PushUpdateKind::Delete(id) => {
      let commit = repo.find_commit(id)?;

      Ok(format!(
        "{} {} {}",
        style("Deleted").red(),
        name,
        style!("(was {})", trim_hash(commit.as_object())?)
      ))
    }
  }
}

fn display_push_rejection(rejection: &PushRejection) -> String {
  format!(
    "{} to push {}: {}",
    style("Failed").red(),
    style(rejection.refname.trim_prefix_opt("refs/remotes/")).cyan(),
    rejection.status
  )
}
