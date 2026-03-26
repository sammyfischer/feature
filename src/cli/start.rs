//! Start subcommand

use git2::Repository;

use crate::cli::{Cli, CliResult, get_current_branch, get_current_commit};
use crate::{cli_err, cli_err_fn, data};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  #[arg(long, default_value = "-")]
  /// The separator to use when joining words
  pub sep: Option<String>,

  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
  /// Words to join together as branch name
  pub words: Vec<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let repo = Repository::open_from_env()?;
    let sep = self.sep.as_ref().unwrap_or(&cli.config.branch_sep);

    let branch_name = self.words.join(sep);
    println!("Creating branch: {}", branch_name);

    let base_name = get_current_branch(&repo)?;
    if !cli.config.bases.contains(&base_name) {
      return Err(cli_err!(
        Generic,
        "Must call start from a base branch. You can modify base branches with the `feature config` command."
      ));
    }

    // find commit to create branch on
    let current_commit = get_current_commit(&repo).map_err(cli_err_fn!(
      Git,
      e,
      "Failed to find current commit: {e}"
    ))?;

    // create branch
    let branch = repo
      .branch(&branch_name, &current_commit, false)
      .map_err(cli_err_fn!(Git, e, "Failed to create branch: {e}"))?;

    // get tree to checkout
    let tree = branch.get().peel_to_tree().map_err(cli_err_fn!(
      Git,
      e,
      "Failed to resolve branch as tree: {e}"
    ))?;

    // checkout branch
    repo
      .checkout_tree(tree.as_object(), None)
      .map_err(cli_err_fn!(Git, e, "Failed to switch to branch: {e}"))?;

    // update HEAD
    repo
      .set_head(&format!("refs/heads/{branch_name}"))
      .map_err(cli_err_fn!(Git, e, "Failed to switch to branch: {e}"))?;

    // getting info to modify config
    let base = repo
      .find_branch(&base_name, git2::BranchType::Local)
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

    let mut config = data::git_config(&repo)?;
    data::set_feature_base(&mut config, &branch_name, &feature_base_name)?;

    Ok(())
  }
}
