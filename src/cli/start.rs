//! Start subcommand

use anyhow::{Result, anyhow};
use console::style;
use git2::{ErrorCode, Repository};

use crate::cli::Cli;
use crate::config::Config;
use crate::templater::{LongVar, ShortVar, Templater};
use crate::util::branch::get_current_branch_name;
use crate::util::get_current_commit;
use crate::{data, open_repo};

const LONG_ABOUT: &str = r"Creates and switches to a new branch.
This command does no checks to validate the branch name or verify that it
doesn't already exist.

Supports several custom formatting options that can be specified in the command
line or config file.";

const NOT_ON_BASE_MSG: &str = r"Must call start from a base branch. You can add a base branche with:

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
#[command(about = "Starts a new feature branch", long_about = LONG_ABOUT)]
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

    let base_name = get_current_branch_name(&repo)?;
    if !cli.config.bases.contains(&base_name) {
      return Err(anyhow!(NOT_ON_BASE_MSG));
    }

    let branch_name = self.build_branch_name(&repo, &cli.config, &base_name)?;

    if self.dry_run {
      print_branch_message(&branch_name, &base_name);
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

    print_branch_message(&branch_name, &base_name);

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
    let sep = self.sep.as_ref().unwrap_or(&config.format.branch_sep);
    let main_part = self.words.join(sep);

    let mut template = self.format.as_ref();
    // use config if cli option isn't specified
    if template.is_none() {
      template = config.format.branch.as_ref();
    }

    // if neither cli nor config specifies a template, just use the main part
    let Some(template) = template else {
      return Ok(main_part);
    };

    if template.is_empty() {
      return Ok(main_part);
    }

    let mut templater = Templater::new()
      .short(ShortVar::eager('s', &main_part))
      .long(LongVar::lazy("user", || {
        repo
          .signature()
          .expect("Failed to get default commit signature")
          .name()
          .expect("Signature name should be valid utf-8")
          .to_string()
      }))
      .long(LongVar::eager("base", base_name))
      .long(LongVar::eager("sep", sep));

    templater.replace(template)
  }
}

#[inline(always)]
fn print_branch_message(branch_name: &str, base_name: &str) {
  println!(
    "{} {} {}",
    style("Created").green(),
    branch_name,
    style(format!("(from {})", base_name)).dim()
  );
}
