use anyhow::{Result, anyhow};
use git2::{Commit, ErrorCode, Repository};

use crate::App;
use crate::util::diff::DiffSummary;
use crate::util::display::{DisplayCommitMessageLevel, DisplayCommitOptions, display_commit};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Show info about a commit")]
pub struct Args {
  /// Hide the diff summary
  #[arg(short = 'S', long)]
  pub no_summary: bool,

  /// How much of the commit message to show
  #[arg(short, long, default_value = "full")]
  pub message: DisplayCommitMessageLevel,

  /// The git revision string, e.g. HEAD^2, commit hash, branch name. See "man gitrevisions".
  pub revision: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let object = state
      .repo
      .revparse_single(self.revision.as_deref().unwrap_or("HEAD"))?;
    let commit = object.peel_to_commit()?;
    println!(
      "{}",
      display_commit(&commit, &DisplayCommitOptions {
        message: self.message,
        time: From::from(&state.config)
      })?
    );

    if !self.no_summary {
      print_summary(&state.repo, &commit)?;
    }

    Ok(())
  }
}

fn print_summary(repo: &Repository, commit: &Commit) -> Result<()> {
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

  let mut diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
  diff.find_similar(None)?;

  let summary = DiffSummary::new(&diff)?;
  if summary.num_files != 0 {
    println!("\n{}", summary);
  }

  Ok(())
}
