use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use git2::{ErrorClass, ErrorCode};

use crate::App;
use crate::cli::display::commit::{DisplayCommitOptions, display_commit};
use crate::cli::display::diff::display_summary;
use crate::cli::display::time::DisplayTimeOptions;
use crate::cli::tag::display_tag;
use crate::cli::term::{is_term, paginate};
use crate::core::NotFoundExt;
use crate::core::diff::{DiffSummary, get_formatted_diff};
use crate::core::project_config::PageWhen;
use crate::core::string::ToStrLossy;
use crate::core::user_config::{CommitMessageLevel, UserConfig};

const LONG_ABOUT: &str = r#"Show info about a commit

For the options "--no-summary", and "--no-patch", an equals sign must be used
to specify a value. If no value is specified, "true" is assumed.

For example:
Use "-S=false" to force the summary to appear.
Use "-S" to force the summary to be hidden."#;

#[derive(clap::Args, Debug)]
#[command(
  about = "Show info about a commit",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Hide the diff summary
  #[arg(short = 'S', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_summary: Option<bool>,

  /// Hide the diff patch
  #[arg(short = 'P', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_patch: Option<bool>,

  /// How much of the commit message to show
  #[arg(short, long, value_name = "LEVEL")]
  message: Option<CommitMessageLevel>,

  /// When to page output
  #[arg(long, value_name = "WHEN")]
  paging: Option<PageWhen>,

  /// Whether to display the object as a tag instead of a commit. This is only
  /// valid for revspecs that resolve to tags.
  #[arg(short, long)]
  tag: bool,

  /// The git revision string, e.g. HEAD^2, commit hash, branch name. See "man
  /// gitrevisions".
  #[arg(value_name = "REVISION", value_hint = ValueHint::Other)]
  revision: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let config = UserConfig::new(&state.repo)?;

    let buf = if self.tag {
      self.show_tag(state, &config)?
    } else {
      self.show_commit(state, &config)?
    };

    // use config value only if it's not explicitly set in the command line
    let paging = self.paging.unwrap_or(config.show_paging()?);
    match (paging, is_term()) {
      (PageWhen::Auto, true) | (PageWhen::Always, _) => paginate(&buf),
      (PageWhen::Auto, false) | (PageWhen::Never, _) => {
        print!("{}", buf.to_str_lossy());
        Ok(())
      }
    }
  }

  fn show_commit(&self, state: &App, config: &UserConfig) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::new();

    let object = state
      .repo
      .revparse_single(self.revision.as_deref().unwrap_or("HEAD"))?;

    let commit = object.peel_to_commit()?;

    writeln!(
      buf,
      "{}",
      display_commit(&commit, &DisplayCommitOptions {
        message: self.message.unwrap_or(config.show_message()?),
        time: DisplayTimeOptions {
          relative: config.format_relative()?,
          fmt: config.format_date()?,
        }
      })?
    )?;

    let parent = commit.parent(0).not_found_ok()?;

    let new_tree = commit.tree()?;
    let old_tree = match parent {
      Some(it) => Some(it.tree()?),
      None => None,
    };

    let mut diff = state
      .repo
      .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
    diff.find_similar(None)?;

    let show_summary = !self.no_summary.unwrap_or(!config.show_summary()?);
    if show_summary {
      let summary = DiffSummary::new(&diff)?;
      if summary.num_files != 0 {
        writeln!(buf, "\n{}", display_summary(&summary))?;
      }
    }

    let show_patch = !self.no_patch.unwrap_or(!config.show_patch()?);
    if show_patch {
      buf.extend_from_slice(&get_formatted_diff(&diff)?);
    }

    Ok(buf)
  }

  fn show_tag(&self, state: &App, config: &UserConfig) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::new();

    let rev = self
      .revision
      .as_ref()
      .context("Must specify a tag to display in tag style")?;

    let tag_ref = state.repo.resolve_reference_from_short_name(rev)?;

    if !tag_ref.is_tag() {
      return Err(anyhow!("{} is not a tag", rev));
    }

    let tag = match tag_ref.peel_to_tag() {
      Ok(tag) => tag,

      Err(e) if e.class() == ErrorClass::Object && e.code() == ErrorCode::InvalidSpec => {
        return Err(anyhow!(
          "{} is not an annotated tag and therefore can't be displayed as a tag object",
          rev
        ));
      }

      Err(e) => {
        return Err(anyhow!(e)).with_context(|| format!("Failed to get {} as a tag object", rev));
      }
    };

    write!(
      buf,
      "{}",
      display_tag(&tag, &DisplayCommitOptions {
        message: self.message.unwrap_or(config.show_message()?),
        time: DisplayTimeOptions {
          relative: config.format_relative()?,
          fmt: config.format_date()?,
        }
      })?
    )?;

    Ok(buf)
  }
}
