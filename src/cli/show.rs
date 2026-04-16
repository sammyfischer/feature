use anyhow::{Result, anyhow};
use git2::ErrorCode;

use crate::util::diff::{DiffSummary, get_formatted_diff};
use crate::util::display::{DisplayCommitMessageLevel, DisplayCommitOptions, display_commit};
use crate::util::term::{is_term, paginate};
use crate::{App, lossy};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Show info about a commit")]
pub struct Args {
  /// Hide the diff summary
  #[arg(short = 'S', long)]
  pub no_summary: bool,

  /// Hide the diff patch
  #[arg(short = 'P', long)]
  pub no_patch: bool,

  /// How much of the commit message to show
  #[arg(short, long, default_value = "full")]
  pub message: DisplayCommitMessageLevel,

  /// When to page output
  #[arg(long, default_value = "auto", value_name = "WHEN")]
  pub paging: PageWhen,

  /// The git revision string, e.g. HEAD^2, commit hash, branch name. See "man gitrevisions".
  pub revision: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum PageWhen {
  #[default]
  Auto,
  Always,
  Never,
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
        message: self.message,
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

    if !self.no_summary {
      let summary = DiffSummary::new(&diff)?;
      if summary.num_files != 0 {
        writeln!(buf, "\n{}", summary)?;
      }
    }

    if !self.no_patch {
      buf.extend_from_slice(&get_formatted_diff(&diff)?);
    }

    match (self.paging, is_term()) {
      (PageWhen::Auto, true) | (PageWhen::Always, _) => paginate(&buf),
      (PageWhen::Auto, false) | (PageWhen::Never, _) => {
        print!("{}", lossy!(&buf));
        Ok(())
      }
    }
  }
}
