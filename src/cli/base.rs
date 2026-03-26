//! Base subcommand

use git2::Repository;

use crate::cli::get_current_branch;
use crate::{CliResult, cli_err, cli_err_fn, data};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// The name of the base branch
  base: String,

  /// The name of the branch whose base is being set. Defaults to current branch
  branch: Option<String>,
}

impl Args {
  pub fn run(&self) -> CliResult {
    let repo = Repository::open_from_env()?;
    let mut config = data::git_config(&repo)?;

    let branch_name = self.branch.clone().unwrap_or(get_current_branch(&repo)?);

    let base = repo
      .find_branch(&self.base, git2::BranchType::Local)
      .map_err(cli_err_fn!(
        Git,
        e,
        "Failed to get reference to base branch: {e}"
      ))?;

    let feature_base_name = {
      // we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = base.upstream().map_err(cli_err_fn!(
        Git,
        e,
        "Failed to get upstream of base branch: {e}"
      ));

      match base_upstream {
        Ok(it) => it
          .get()
          .name()
          .ok_or(cli_err!(Git, "Failed to get upstream name of base branch"))?
          .to_string(),

        // if there is no upstream, we can just use the actual base branch
        Err(_) => base
          .get()
          .name()
          .ok_or(cli_err!(Git, "Failed to get full name of base branch"))?
          .to_string(),
      }
    };

    data::set_feature_base(&mut config, &branch_name, &feature_base_name)?;

    Ok(())
  }
}
