//! Base subcommand

use anyhow::{Context, Result, anyhow};

use crate::util::branch::{get_current_branch_name, get_upstream, name_to_branch};
use crate::{data, open_repo};

const LONG_ABOUT: &str = r#"Tells feature which base corresponds to a branch.

Feature automatically tracks base branches when you use "feature start", but if
you use other tools you'll have to tell feature which one to use. Base branches
can't be quickly or reliably determined, so you will have to specify it
manually for some feature commands to work."#;

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
  pub fn run(&self) -> Result<()> {
    let repo = open_repo!();
    let mut config = data::git_config(&repo)?;

    let branch_name = match &self.branch {
      Some(it) => it,
      None => &get_current_branch_name(&repo)?.context(NOT_ON_BRANCH_MSG)?,
    };

    let base = name_to_branch(&repo, &self.base)
      .with_context(|| format!("Failed to get base branch {}", &self.base))?;

    let feature_base_name = {
      // we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = get_upstream(&base)
        .with_context(|| format!("Failed to check if {} has an upstream", &self.base))?;

      match base_upstream {
        Some(it) => it
          .get()
          .name()
          .ok_or(anyhow!("Failed to get upstream name of base branch"))?
          .to_string(),

        // if there is no upstream, we can just use the actual base branch
        None => base
          .get()
          .name()
          .ok_or(anyhow!("Failed to get full name of base branch"))?
          .to_string(),
      }
    };

    data::set_feature_base(&mut config, branch_name, &feature_base_name)?;

    Ok(())
  }
}
