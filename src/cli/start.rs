//! Start subcommand

use anyhow::{Context, Result};
use clap::ValueHint;
use console::style;

use crate::templater::{LongVar, ShortVar, Templater};
use crate::util::branch::switch;
use crate::util::branch_meta::BranchMeta;
use crate::util::string::ToStrLossyOwned;
use crate::{App, data};

const LONG_ABOUT: &str = r#"Creates and switches to a new branch.

All trailing args are joined together to form the branch name. To avoid
unexpected behavior, you should specify all cli options first, then add branch
name args.

To be more explicit, you can separate WORDS with "--":
• feature start --sep='_' -- name of the branch

Supports several custom formatting options that can be specified in the command
line or config file."#;

const FORMAT_HELP_MSG: &str = r#"Template replacements (in order):
  %%      -> a literal '%'
  %(user) -> feature.user (set with "git config feature.user <username>")
  %(base) -> base branch name
  %(sep)  -> the separator used to join WORDS
  %s      -> WORDS joined by the separator"#;

const NOT_ON_BRANCH_MSG: &str = r#"Not currently on a branch! You can switch to a branch or specify one manually
with the "--from <BRANCH>" option."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "branch",
  about = "Starts a new feature branch",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Display the branch name, after joining args and performing template
  /// replacements
  #[arg(long)]
  pub dry_run: bool,

  /// The separator to use when joining words
  #[arg(long, value_hint = ValueHint::Other)]
  pub sep: Option<String>,

  /// Format specifier for branch name
  #[arg(long, visible_alias = "fmt", long_help = FORMAT_HELP_MSG, value_hint = ValueHint::Other)]
  pub format: Option<String>,

  /// Which base branch to start from
  #[arg(long, visible_alias = "base", value_name = "BRANCH", value_hint = ValueHint::Other)]
  pub from: Option<String>,

  /// Whether to stay on the current branch
  #[arg(short, long)]
  pub stay: bool,

  /// Words to join together as branch name
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true, value_hint = ValueHint::Other)]
  pub words: Vec<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let base = match &self.from {
      Some(base_name) => BranchMeta::from_name_dwim(&state.repo, base_name)?
        .with_context(|| format!("Branch not found: {}", base_name))?,
      None => BranchMeta::current(&state.repo)?.context(NOT_ON_BRANCH_MSG)?,
    };

    let branch_name = self.build_branch_name(state, base.name())?;

    if self.dry_run {
      println!(
        "{}",
        style("Running in dry-run mode, no branch will be created").dim()
      );
      display_branch_creation(&branch_name, base.name());
      return Ok(());
    }

    let start_commit = base.resolve(&state.repo)?.peel_to_commit()?;

    // create branch
    let branch = state
      .repo
      .branch(&branch_name, &start_commit, false)
      .context("Failed to create branch")?;

    display_branch_creation(&branch_name, base.name());

    // switch to branch if user didn't specify --stay
    if !self.stay {
      let meta = BranchMeta::from_branch(&branch)?;
      switch(&state.repo, &meta)?;
    }

    // set feature-base in config
    let feature_base_name = {
      // ideally we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = base.upstream(&state.repo)?;

      match base_upstream {
        Some(it) => it.get().name_bytes().to_str_lossy_owned(),

        // if there is no upstream, we can just use the actual base branch
        None => base.refname().to_string(),
      }
    };

    let mut config = state.repo.config()?;
    data::set_feature_base(&mut config, &branch_name, &feature_base_name)?;

    Ok(())
  }

  fn build_branch_name(&self, state: &App, base_name: &str) -> Result<String> {
    let sep = self.sep.as_ref().unwrap_or(&state.config.branch.sep);
    let main_part = self.words.join(sep);

    let mut template = self.format.as_ref();
    // use config if cli option isn't specified
    if template.is_none() {
      template = state.config.branch.template.as_ref();
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
        let config = state.repo.config()?.snapshot()?;
        data::get_feature_user(&config)?
          .context("No value for feature.user. Set it with \"git config feature.user <username>\".")
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
