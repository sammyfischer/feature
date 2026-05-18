use anyhow::{Context, Result, anyhow};
use clap::ValueHint;

use crate::util::branch::fetch_upstream_branch;
use crate::util::branch_meta::BranchMeta;
use crate::{App, await_child, data, git, style};

const LONG_ABOUT: &str = r"Rebases this branch onto its base. The available commands are similar to a git
rebase.";

const NO_BASE_MSG: &str = r#"No base branch found. You can:

• Manually specify the base branch: "feature update <BASE_BRANCH>"
• Set the base branch permanently: "feature base <BASE_BRANCH>""#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Updates this branch with its base",
  long_about = LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true)]
pub struct Args {
  /// Output which base branch will be used, but don't perform the rebase
  #[arg(long)]
  dry_run: bool,

  /// Continue an active rebase
  #[arg(short, long)]
  r#continue: bool,

  /// Abort an active rebase
  #[arg(short, long)]
  abort: bool,

  /// Skip applying current commit in an active rebase
  #[arg(short, long)]
  skip: bool,

  /// The name of the base branch to use.
  #[arg(value_name = "BRANCH", value_hint = ValueHint::Other)]
  base: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    if self.r#continue {
      return await_child!(git!("rebase", "--continue").spawn()?, "Git");
    }
    if self.abort {
      return await_child!(git!("rebase", "--abort").spawn()?, "Git");
    }
    if self.skip {
      return await_child!(git!("rebase", "--skip").spawn()?, "Git");
    }

    let branch =
      BranchMeta::current(&state.repo)?.context("Not currently on a branch! Nothing to update.")?;

    let base = match &self.base {
      Some(base_name) => BranchMeta::from_name_dwim(&state.repo, base_name)?
        .ok_or(anyhow!("Branch not found: {}", base_name))?,
      None => data::get_feature_base(&state.repo, branch.name())?.ok_or(anyhow!(NO_BASE_MSG))?,
    };

    if self.dry_run {
      println!("Using base: {}", base.name());
      return Ok(());
    }

    // if base is a remote, fetch the latest
    if base.is_remote() {
      fetch_upstream_branch(&state.repo, &base)?;
      println!("{}", style!("Fetched {}", base.name()).dim());
    }

    await_child!(git!("rebase", base.refname()).spawn()?, "Git")
  }
}
