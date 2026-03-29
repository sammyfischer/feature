//! Start subcommand

use anyhow::{Context, Result, anyhow};
use git2::{ErrorCode, Repository};

use crate::cli::{Cli, get_current_branch, get_current_commit};
use crate::config::Config;
use crate::{data, open_repo};

const NOT_ON_BASE_MSG: &str = r"Must call start from a base branch. You can modify base branches with:

`feature config append bases <BRANCH_NAME>`";

const EMPTY_REPO_MSG: &str =
  r"Cannot call start on an empty repository. Create at least one commit first.";

const FORMAT_HELP_MSG: &str = r"Template replacements (in order):
  %%      -> a literal '%'
  %(user) -> user.name found in git config
  %(base) -> base branch name
  %(sep)  -> the separator used to join WORDS
  %s      -> WORDS joined by the separator";

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// The separator to use when joining words
  #[arg(long)]
  pub sep: Option<String>,

  /// Format specifier for branch name
  #[arg(long, visible_alias = "fmt", long_help = FORMAT_HELP_MSG)]
  pub format: Option<String>,

  /// Just print the branch name, after joining args and performing template replacements
  #[arg(long)]
  pub dry_run: bool,

  /// Words to join together as branch name
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
  pub words: Vec<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();

    let base_name = get_current_branch(&repo)?;
    if !cli.config.bases.contains(&base_name) {
      return Err(anyhow!(NOT_ON_BASE_MSG));
    }

    let branch_name = self.build_branch_name(&repo, &cli.config, &base_name)?;
    println!("Creating branch: {}", branch_name);

    if self.dry_run {
      return Ok(());
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

  fn build_branch_name(
    &self,
    repo: &Repository,
    config: &Config,
    base_name: &str,
  ) -> Result<String> {
    let sep = self.sep.as_ref().unwrap_or(&config.branch_sep);

    // unique part of the branch name
    let main_part = self.words.join(sep);

    let template = self.format.as_ref().unwrap_or(&config.branch_format);

    // TODO: lazily evaluate on first replacement
    // would involve:
    // - making replacements occur in a loop, scan across template once
    // - push characters to new string, replace patterns as they're encountered
    // - outside the loop, store a string buffer to cache the evaluated value as an option
    //   - None => evaluate, Some => use cached value
    let signature = repo.signature().context("Failed to get git user name")?;
    let username = signature
      .name()
      .expect("Git user name should be valid utf-8");

    // "%%" must be replaced first, for the others order doesn't matter
    Ok(
      template
        .replace(r"%%", "%")
        .replace(r"%(user)", username)
        .replace(r"%(base)", base_name)
        .replace(r"%(sep)", sep)
        .replace(r"%s", &main_part),
    )
  }
}
