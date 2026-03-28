//! Start subcommand

use anyhow::{Result, anyhow};
use git2::ErrorCode;

use crate::cli::{Cli, get_current_branch, get_current_commit};
use crate::{data, open_repo};

const NOT_ON_BASE_MSG: &str = r"Must call start from a base branch. You can modify base branches with:

`feature config append bases <BRANCH_NAME>`";

const EMPTY_REPO_MSG: &str =
  r"Cannot call start on an empty repository. Create at least one commit first.";

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
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();
    let sep = self.sep.as_ref().unwrap_or(&cli.config.branch_sep);

    let branch_name = self.words.join(sep);
    println!("Creating branch: {}", branch_name);

    let base_name = get_current_branch(&repo)?;
    if !cli.config.bases.contains(&base_name) {
      return Err(anyhow!(NOT_ON_BASE_MSG));
    }

    // find commit to create branch on
    let current_commit = get_current_commit(&repo)
      .expect("Failed to find current commit")
      .ok_or(anyhow!(EMPTY_REPO_MSG))?;

    // create branch
    let branch = repo
      .branch(&branch_name, &current_commit, false)
      .expect("Failed to create branch");

    // get tree to checkout
    let tree = branch
      .get()
      .peel_to_tree()
      .expect("Failed to get branch as tree to checkout");

    // checkout branch
    repo
      .checkout_tree(tree.as_object(), None)
      .expect("Failed to switch to branch");

    // update HEAD
    repo
      .set_head(&format!("refs/heads/{}", branch_name))
      .unwrap_or_else(|_| {
        panic!(
          "Failed to update HEAD to new branch {0}. Run: \
          \
          `git switch {0}`",
          branch_name
        )
      });

    // getting info to modify config
    let base = repo
      .find_branch(&base_name, git2::BranchType::Local)
      .unwrap_or_else(|_| panic!("Failed to get reference to base branch {}", base_name));

    let feature_base_name = {
      // we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = match base.upstream() {
        Ok(it) => Some(it),
        Err(e) if e.code() == ErrorCode::NotFound => None,
        Err(e) => {
          return Err(
            anyhow!(e).context(format!("Failed to check if {} has an upstream", base_name)),
          );
        }
      };

      match base_upstream {
        Some(it) => it
          .get()
          .name()
          .expect("Failed to get upstream name of base branch")
          .to_string(),

        // if there is no upstream, we can just use the actual base branch
        None => base
          .get()
          .name()
          .expect("Failed to get full refname of base branch")
          .to_string(),
      }
    };

    let mut config = data::git_config(&repo)?;
    data::set_feature_base(&mut config, &branch_name, &feature_base_name)?;

    Ok(())
  }
}
