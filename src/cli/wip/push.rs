use anyhow::{Result, anyhow};
use console::style;

use crate::App;
use crate::cli::advice::NOT_ON_BRANCH_MSG;
use crate::cli::display::commit::{DisplayCommitOptions, display_commit};
use crate::cli::display::diff::display_summary;
use crate::cli::display::time::DisplayTimeOptions;
use crate::core::diff::DiffSummary;
use crate::core::user_config::{CommitMessageLevel, UserConfig};
use crate::core::wip::WipList;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Pushes a new wip to a branch")]
pub struct PushArgs {
  /// Push only staged changes, instead of the entire workdir
  #[arg(short, long)]
  staged: bool,

  /// Include untracked files
  #[arg(short, long)]
  untracked: bool,

  /// Keep changes in working directory
  #[arg(short, long)]
  keep: bool,

  /// Which branch to push to
  #[arg(short, long)]
  branch: Option<String>,

  /// Wip message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  message: Vec<String>,
}

impl PushArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let branch = match &self.branch {
      Some(name) => name.to_owned(),
      None => {
        let head = repo.head()?;
        if !head.is_branch() {
          return Err(anyhow!(NOT_ON_BRANCH_MSG));
        }

        head.shorthand()?.to_string()
      }
    };

    let msg = if self.message.is_empty() {
      None
    } else {
      Some(self.message.join(" "))
    };

    let mut list = WipList::from_branch(repo, branch.clone())?;
    let wip = list.push(repo, msg.as_deref(), self.staged, self.untracked, self.keep)?;
    let commit = repo.find_commit(wip.commit())?;

    println!(
      "{} changes to {}",
      style("Pushed").green(),
      style(&branch).cyan()
    );

    println!(
      "{}",
      display_commit(&commit, &DisplayCommitOptions {
        time: DisplayTimeOptions {
          relative: false,
          fmt: String::new(),
        },
        message: CommitMessageLevel::Full,
      },)?
    );

    let parent = commit
      .parent(0)
      .expect("Failed to get first parent of wip commit");

    let mut diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&commit.tree()?), None)?;
    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;
    println!(
      "\n{}",
      display_summary(&summary, UserConfig::new(repo)?.nerdfont()?)
    );

    Ok(())
  }
}
