use anyhow::{Result, anyhow};
use git2::ErrorCode;

use crate::config::PageWhen;
use crate::util::diff::{DiffSummary, get_formatted_diff};
use crate::util::display::{DisplayCommitMessageLevel, DisplayCommitOptions, display_commit};
use crate::util::term::{is_term, paginate};
use crate::{App, lossy};

const LONG_ABOUT: &str = r#"Show info about a commit

For the options "--no-summary", and "--no-patch", an equals sign must be used
to specify a value. If no value is specified, "true" is assumed.

For example:
Use "-S=false" to force the summary to appear.
Use "-S" to force the summary to be hidden."#;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Show info about a commit", long_about = LONG_ABOUT)]
pub struct Args {
  /// Hide the diff summary
  #[arg(short = 'S', long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_summary: Option<bool>,

  /// Hide the diff patch
  #[arg(short = 'P', long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_patch: Option<bool>,

  /// How much of the commit message to show
  #[arg(short, long)]
  pub message: Option<DisplayCommitMessageLevel>,

  /// When to page output
  #[arg(long, value_name = "WHEN")]
  pub paging: Option<PageWhen>,

  /// The git revision string, e.g. HEAD^2, commit hash, branch name. See "man gitrevisions".
  pub revision: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
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
        message: self.message.unwrap_or(state.config.show.message),
        time: From::from(&state.config)
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

    let show_summary = !self.no_summary.unwrap_or(!state.config.show.summary);
    if show_summary {
      let summary = DiffSummary::new(&diff)?;
      if summary.num_files != 0 {
        writeln!(buf, "\n{}", summary)?;
      }
    }

    let show_patch = !self.no_patch.unwrap_or(!state.config.show.patch);
    if show_patch {
      buf.extend_from_slice(&get_formatted_diff(&diff)?);
    }

    // use config value only if it's not explicitly set in the command line
    let paging = self.paging.unwrap_or(state.config.show.paging);
    match (paging, is_term()) {
      (PageWhen::Auto, true) | (PageWhen::Always, _) => paginate(&buf),
      (PageWhen::Auto, false) | (PageWhen::Never, _) => {
        print!("{}", lossy!(&buf));
        Ok(())
      }
    }
  }
}
