use anyhow::{Context, Result, anyhow};
use console::style;
use git2::build::CheckoutBuilder;
use git2::{Commit, DiffOptions};

use crate::App;
use crate::cli::advice::NOT_ON_BRANCH_MSG;
use crate::cli::display::commit::{DisplayCommitOptions, display_commit};
use crate::cli::display::diff::display_summary;
use crate::cli::display::time::DisplayTimeOptions;
use crate::core::NotFoundExt;
use crate::core::branch_info::BranchInfo;
use crate::core::diff::DiffSummary;
use crate::core::user_config::{CommitMessageLevel, UserConfig};
use crate::core::wip::get_wip_refname;

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
    let head = repo.head()?;

    let branch = match &self.branch {
      Some(name) => BranchInfo::from_name_dwim(repo, name)?
        .with_context(|| format!("Failed to find branch: {}", name))?,
      None => {
        if !head.is_branch() {
          return Err(anyhow!(NOT_ON_BRANCH_MSG));
        }

        BranchInfo::from_reference(&head.resolve()?)?
      }
    };

    let parent = branch.resolve(repo)?.peel_to_commit()?;

    let sig = repo.signature()?;
    let msg = if self.message.is_empty() {
      // use parent commit message
      format!("WIP: {}", match parent.summary()? {
        // unwrap
        Some(it) => it,
        // or else
        None => parent.message()?,
      })
    } else {
      self.message.join(" ")
    };

    // build tree of changes
    let tree = if self.staged {
      let mut index = repo.index()?;
      let tree_id = index.write_tree()?;
      repo.find_tree(tree_id)?
    } else {
      let base_tree = head.peel_to_tree()?;

      let mut opts = DiffOptions::new();
      opts.include_untracked(self.untracked);
      opts.recurse_untracked_dirs(self.untracked);
      let diff = repo.diff_tree_to_workdir(Some(&base_tree), Some(&mut opts))?;

      let mut index = repo
        .apply_to_tree(&base_tree, &diff, None)
        .context("Failed to build wip changes")?;

      let tree_id = index.write_tree_to(repo)?;
      repo.find_tree(tree_id)?
    };

    // TODO: add more parents to diff between staged, unstaged, and untracked files
    // in wip commit
    let parents: Vec<Commit> = vec![parent];

    // create wip commit
    let commit_id = repo.commit(
      None,
      &sig,
      &sig,
      &msg,
      &tree,
      // map from Vec<Commit> to Vec<&Commit>
      &parents.iter().collect::<Vec<_>>(),
    )?;

    let wip = repo.find_commit(commit_id)?;

    // create/update reference
    let wip_refname = get_wip_refname(branch.name());
    match repo.find_reference(&wip_refname).not_found_ok()? {
      // a wip ref exists for this branch, update it
      Some(mut wip_ref) => {
        wip_ref.set_target(commit_id, &msg)?;
      }

      // no wip ref exists for this branch, create it
      None => {
        repo.reference(&wip_refname, commit_id, false, &msg)?;

        // create the wip's reflog (not done automatically)
        let mut reflog = repo.reflog(&wip_refname)?;
        reflog.append(commit_id, &sig, Some(&msg))?;
        reflog.write()?;
      }
    }

    // remove changes from workdir
    if !self.keep {
      if self.staged {
        // existing reference to head hasn't changed
        let base_tree = head.peel_to_tree()?;

        let mut opts = DiffOptions::new();
        opts.include_untracked(self.untracked);
        opts.recurse_untracked_dirs(self.untracked);
        let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;

        // index built from head + unstaged changes
        // `git stash --staged` can fail to apply here. it keeps the stash entry but
        // fails to remove changes from workdir
        let mut index = repo
          .apply_to_tree(&base_tree, &diff, None)
          .context("Wip was created, but changes cannot be removed from working directory")?;

        // checkout to update workdir
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.checkout_index(Some(&mut index), Some(&mut checkout))?;

        // update index to match head
        let mut index = repo.index()?;
        index.read_tree(&repo.head()?.peel_to_tree()?)?;
        index.write()?;
      } else {
        // just reset to head
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.reset(
          // existing reference to head hasn't changed
          head.peel_to_commit()?.as_object(),
          git2::ResetType::Hard,
          Some(&mut checkout),
        )?;
      }
    }

    println!(
      "{} changes to {}",
      style("Pushed").green(),
      style(branch.name()).cyan()
    );

    println!(
      "{}",
      display_commit(&wip, &DisplayCommitOptions {
        time: DisplayTimeOptions {
          relative: false,
          fmt: String::new(),
        },
        message: CommitMessageLevel::Full,
      },)?
    );

    let parent = wip
      .parent(0)
      .expect("Failed to get first parent of wip commit");
    let mut diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&wip.tree()?), None)?;

    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;
    println!(
      "\n{}",
      display_summary(&summary, UserConfig::new(repo)?.nerdfont()?)
    );

    Ok(())
  }
}
