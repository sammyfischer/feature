//! Base subcommand

use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use git2::Branch;

use crate::App;
use crate::core::branch_info::BranchInfo;
use crate::core::string::ToStrLossyOwned;
use crate::core::{NotFoundExt, user_config};

const LONG_ABOUT: &str = r#"Tells feature which base corresponds to a branch.

Feature automatically tracks base branches when you use "feature start", but if
you use another tool to create a branch you'll have to tell feature which one to
use. Base branches can't be quickly or reliably determined, so you will have to
specify it manually for some feature commands to work."#;

const NOT_ON_BRANCH_MSG: &str = r"Not currently on a branch! You can switch to a branch or specify one manually
with the --branch option.";

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Tell feature which base another branch belongs to",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct Args {
  /// The name of the base branch
  #[arg(value_name = "BRANCH", value_hint = ValueHint::Other)]
  base: String,

  /// The name of the branch whose base is being set. Defaults to current branch
  #[arg(long, value_name = "BRANCH", value_hint = ValueHint::Other)]
  branch: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let branch = match &self.branch {
      Some(branch_name) => BranchInfo::from_name_dwim(&state.repo, branch_name)?
        .ok_or(anyhow!("Branch not found: {}", branch_name))?,
      None => BranchInfo::current(&state.repo)?.context(NOT_ON_BRANCH_MSG)?,
    };

    let base = BranchInfo::from_name_dwim(&state.repo, &self.base)?
      .ok_or(anyhow!("Branch not found: {}", self.base))?;

    let feature_base_name = {
      // we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = Branch::wrap(base.resolve(&state.repo)?)
        .upstream()
        .not_found_ok()
        .with_context(|| format!("Failed to check if {} has an upstream", &self.base))?;

      match base_upstream {
        Some(upstream) => upstream.get().name_bytes().to_str_lossy_owned(),

        // if there is no upstream, we can just use the actual base branch
        None => base.refname().to_string(),
      }
    };

    // get again as writable config
    let mut config = state.repo.config()?;
    user_config::set_feature_base(&mut config, branch.name(), &feature_base_name)?;

    Ok(())
  }
}
