//! Base subcommand

use anyhow::{Context, Result, anyhow};
use git2::Branch;

use crate::util::branch::{get_head, get_upstream};
use crate::util::branch_meta::BranchMeta;
use crate::{App, data, lossy};

const LONG_ABOUT: &str = r#"Tells feature which base corresponds to a branch.

Feature automatically tracks base branches when you use "feature start", but if
you use another tool to create a branch you'll have to tell feature which one to
use. Base branches can't be quickly or reliably determined, so you will have to
specify it manually for some feature commands to work."#;

const NOT_ON_BRANCH_MSG: &str = r"Not currently on a branch! You can switch to a branch or specify one manually
with the --branch option.";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Tell feature which base another branch belongs to", long_about = LONG_ABOUT)]
pub struct Args {
  /// The name of the base branch
  base: String,

  /// The name of the branch whose base is being set. Defaults to current branch
  #[arg(long)]
  branch: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let mut config = state.repo.config()?;

    let branch = match &self.branch {
      Some(branch_name) => BranchMeta::from_name_dwim(&state.repo, branch_name)?
        .ok_or(anyhow!("Branch not found: {}", branch_name))?,
      None => {
        let head = get_head(&state.repo)?.ok_or(anyhow!("No commits yet"))?;
        if !head.is_branch() {
          return Err(anyhow!(NOT_ON_BRANCH_MSG));
        }
        BranchMeta::from_reference(head)?
      }
    };

    let base = BranchMeta::from_name_dwim(&state.repo, &self.base)?
      .ok_or(anyhow!("Branch not found: {}", self.base))?;

    let feature_base_name = {
      // we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = get_upstream(&Branch::wrap(base.resolve(&state.repo)?))
        .with_context(|| format!("Failed to check if {} has an upstream", &self.base))?;

      match base_upstream {
        Some(upstream) => lossy!(upstream.get().name_bytes()).to_string(),

        // if there is no upstream, we can just use the actual base branch
        None => base.refname().to_string(),
      }
    };

    data::set_feature_base(&mut config, branch.name(), &feature_base_name)?;

    Ok(())
  }
}
