use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use git2::{ErrorClass, ErrorCode};

use crate::config::PageWhen;
use crate::util::diff::{DiffSummary, get_formatted_diff};
use crate::util::display::{
  DisplayCommitMessageLevel,
  DisplayCommitOptions,
  DisplayTimeOptions,
  display_commit,
  display_tag,
};
use crate::util::string::ToStrLossy;
use crate::util::term::{is_term, paginate};
use crate::{App, data};

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
  message: Option<DisplayCommitMessageLevel>,

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
    let config = state.repo.config()?.snapshot()?;

    let buf = if self.tag {
      self.show_tag(state, &config)?
    } else {
      self.show_commit(state, &config)?
    };

    // use config value only if it's not explicitly set in the command line
    let paging = self.paging.unwrap_or(data::get_show_paging(&config)?);
    match (paging, is_term()) {
      (PageWhen::Auto, true) | (PageWhen::Always, _) => paginate(&buf),
      (PageWhen::Auto, false) | (PageWhen::Never, _) => {
        print!("{}", buf.to_str_lossy());
        Ok(())
      }
    }
  }

  fn show_commit(&self, state: &App, git_config: &git2::Config) -> Result<Vec<u8>> {
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
        message: self.message.unwrap_or(data::get_show_message(git_config)?),
        time: DisplayTimeOptions {
          relative: data::get_format_relative(git_config)?,
          fmt: data::get_format_date(git_config)?
        }
      })?
    )?;

    let parent = match commit.parent(0) {
      Ok(it) => Some(it),
      Err(e) if e.code() == ErrorCode::NotFound => None,
      Err(e) => return Err(anyhow!(e)),
    };

    let new_tree = commit.tree()?;
    let old_tree = match parent {
      Some(it) => Some(it.tree()?),
      None => None,
    };

    let mut diff = state
      .repo
      .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
    diff.find_similar(None)?;

    let show_summary = !self
      .no_summary
      .unwrap_or(!data::get_show_summary(git_config)?);
    if show_summary {
      let summary = DiffSummary::new(&diff)?;
      if summary.num_files != 0 {
        writeln!(buf, "\n{}", summary)?;
      }
    }

    let show_patch = !self.no_patch.unwrap_or(!data::get_show_patch(git_config)?);
    if show_patch {
      buf.extend_from_slice(&get_formatted_diff(&diff)?);
    }

    Ok(buf)
  }

  fn show_tag(&self, state: &App, git_config: &git2::Config) -> Result<Vec<u8>> {
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
        message: self.message.unwrap_or(data::get_show_message(git_config)?),
        time: DisplayTimeOptions {
          relative: data::get_format_relative(git_config)?,
          fmt: data::get_format_date(git_config)?
        }
      })?
    )?;

    Ok(buf)
  }
}
