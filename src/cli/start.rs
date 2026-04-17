//! Start subcommand

use anyhow::{Context, Result, anyhow};
use console::style;

use crate::templater::{LongVar, ShortVar, Templater};
use crate::util::branch::{
  branch_to_commit,
  get_current_branch_name,
  get_upstream,
  name_to_branch,
};
use crate::util::get_signature;
use crate::{App, data, lossy};

const LONG_ABOUT: &str = r"Creates and switches to a new branch.
This command does no checks to validate the branch name or verify that it
doesn't already exist.

Supports several custom formatting options that can be specified in the command
line or config file.";

const FORMAT_HELP_MSG: &str = r"Template replacements (in order):
  %%      -> a literal '%'
  %(user) -> user.name found in git config
  %(base) -> base branch name
  %(sep)  -> the separator used to join WORDS
  %s      -> WORDS joined by the separator";

const NOT_ON_BRANCH_MSG: &str = r"Not currently on a branch! You can switch to a branch or specify one manually
with the --from option.";

const NOT_ON_BASE_MSG: &str = r"Must start from a base branch. You can add a base branch with:

`feature config append bases <BRANCH_NAME>`";

const EMPTY_REPO_MSG: &str =
  r"Cannot call start on an empty repository. Create at least one commit first.";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Starts a new feature branch", long_about = LONG_ABOUT)]
pub struct Args {
  /// Display the branch name, after joining args and performing template replacements
  #[arg(long)]
  pub dry_run: bool,

  /// The separator to use when joining words
  #[arg(long)]
  pub sep: Option<String>,

  /// Format specifier for branch name
  #[arg(long, visible_alias = "fmt", long_help = FORMAT_HELP_MSG)]
  pub format: Option<String>,

  /// Which base branch to start from
  #[arg(long, value_name = "BRANCH")]
  pub from: Option<String>,

  /// Words to join together as branch name
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
  pub words: Vec<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let base_name = self
      .from
      .as_ref()
      .unwrap_or(&get_current_branch_name(&state.repo)?.context(NOT_ON_BRANCH_MSG)?)
      .clone();

    if !state.config.bases.contains(&base_name) {
      return Err(anyhow!(NOT_ON_BASE_MSG));
    }

    let base = name_to_branch(&state.repo, &base_name)?;
    let branch_name = self.build_branch_name(state, &base_name)?;

    if self.dry_run {
      display_branch_creation(&branch_name, &base_name);
      return Ok(());
    }

    // find commit to create branch on
    let start_commit = branch_to_commit(&base)?.context(EMPTY_REPO_MSG)?;

    // create branch
    let branch = state
      .repo
      .branch(&branch_name, &start_commit, false)
      .context("Failed to create branch")?;

    display_branch_creation(&branch_name, &base_name);

    // get tree to checkout
    let tree = branch
      .get()
      .peel_to_tree()
      .context("Failed to get branch as tree to checkout")?;

    // checkout branch
    state
      .repo
      .checkout_tree(tree.as_object(), None)
      .context("Failed to switch to branch")?;

    // update HEAD
    state
      .repo
      .set_head(&format!("refs/heads/{}", branch_name))
      .with_context(|| {
        format!(
          "Failed to update HEAD to new branch {0}. Run: \
          \
          `git switch {0}`",
          branch_name
        )
      })?;

    let feature_base_name = {
      // we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = get_upstream(&base)?;

      match base_upstream {
        Some(it) => lossy!(it.get().name_bytes()).to_string(),

        // if there is no upstream, we can just use the actual base branch
        None => lossy!(base.get().name_bytes()).to_string(),
      }
    };

    let mut config = data::git_config(&state.repo)?;
    data::set_feature_base(&mut config, &branch_name, &feature_base_name)?;

    Ok(())
  }

  fn build_branch_name(&self, state: &App, base_name: &str) -> Result<String> {
    let sep = self.sep.as_ref().unwrap_or(&state.config.format.branch_sep);
    let main_part = self.words.join(sep);

    let mut template = self.format.as_ref();
    // use config if cli option isn't specified
    if template.is_none() {
      template = state.config.format.branch.as_ref();
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
        lossy!(
          get_signature(&state.repo)
            .expect("Failed to get default commit signature")
            .expect("Specify a username with git config user.name <name>")
            .name_bytes()
        )
        .to_string()
      }))
      .long(LongVar::eager("base", base_name))
      .long(LongVar::eager("sep", sep));

    templater.replace(template)
  }
}

#[inline]
fn display_branch_creation(branch_name: &str, base_name: &str) {
  println!(
    "{} {} {}",
    style("Created").green(),
    branch_name,
    style(format!("(from {})", base_name)).dim()
  );
}
