use anyhow::{Context, Result, anyhow};
use console::style;

use super::push::PushCheckStatus;
use crate::cli::push::{check_base, check_upstream};
use crate::util::branch_meta::BranchMeta;
use crate::{App, data};

const LONG_ABOUT: &str = r"Performs checks on a branch similar to the push/prune commands.

Side effect: this command will try to fetch the latest upstream and base.";

const NOT_ON_BRANCH_MSG: &str = r"Not currently on a branch! You can switch to a branch or specify one manually
as the last argument.";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Checks merge-ability of a branch. May fetch remote branches", long_about = LONG_ABOUT)]
pub struct Args {
  /// The base to use for the branch
  #[arg(long, value_name = "BRANCH-ISH")]
  pub base: Option<String>,

  /// The branch to check
  #[arg(value_name = "BRANCH-ISH")]
  pub branch: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let branch = match &self.branch {
      Some(branch_name) => BranchMeta::from_name_dwim(&state.repo, branch_name)?
        .ok_or(anyhow!("Branch not found: {}", branch_name))?,
      None => BranchMeta::current(&state.repo)?.context(NOT_ON_BRANCH_MSG)?,
    };

    println!("Checking {}…", style(branch.name()).cyan());

    if let Some(upstream) = branch.upstream(&state.repo)? {
      let upstream = BranchMeta::from_branch(&upstream)?;
      println!();

      let status = match check_upstream(&state.repo, &branch, Some(&upstream), false)? {
        // shouldn't occur at this point
        PushCheckStatus::NoBranch => "non-existent".to_string(),
        PushCheckStatus::Forced => "ignored".to_string(),

        PushCheckStatus::UpToDate | PushCheckStatus::Ahead => {
          style("up to date").green().to_string()
        }

        PushCheckStatus::Behind => format!(
          "{}\n  This is automatic when pushing or syncing",
          style("fast-forwardable").green()
        ),

        PushCheckStatus::Diverged => format!(
          "{}\n  You'll have to bring in the upstream changes before pushing",
          style("diverged").red()
        ),
      };

      println!(
        "{} {}: {}",
        style("Against upstream").blue(),
        upstream.name(),
        status
      );
    };

    let base = match self.base.as_ref() {
      Some(base_name) => BranchMeta::from_name_dwim(&state.repo, base_name)?,
      None => data::get_feature_base(&state.repo, branch.name())?,
    };

    if let Some(base) = base {
      println!();

      let status = match check_base(&state.repo, &branch, Some(&base), false)? {
        // shouldn't occur at this point
        PushCheckStatus::NoBranch => "non-existent".to_string(),
        PushCheckStatus::Forced => "ignored".to_string(),

        PushCheckStatus::UpToDate | PushCheckStatus::Ahead => {
          style("up to date").green().to_string()
        }

        PushCheckStatus::Behind => format!(
          "{}\n  This is automatic when pushing or syncing",
          style("fast-forwardable").green()
        ),

        PushCheckStatus::Diverged => format!(
          "{}\n  You'll have to bring in the upstream changes before pushing",
          style("diverged").red()
        ),
      };

      println!(
        "{} {}: {}",
        style("Against base").magenta(),
        base.name(),
        status
      );
    }

    Ok(())
  }
}
