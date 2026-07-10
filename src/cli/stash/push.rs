use anyhow::{Context, Result, anyhow};
use console::style;
use git2::build::CheckoutBuilder;
use git2::{Commit, DiffOptions, ErrorCode};

use crate::App;
use crate::util::advice::NOT_ON_BRANCH_MSG;
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::DiffSummary;
use crate::util::display::{
  DisplayCommitMessageLevel,
  DisplayCommitOptions,
  DisplayTimeOptions,
  display_commit,
};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Pushes a new stash on this branch")]
pub struct PushArgs {
  /// Stash only staged changes, instead of the entire workdir
  #[arg(short, long)]
  staged: bool,

  /// Include untracked files
  #[arg(short, long)]
  untracked: bool,

  /// Keep stashed files in working directory
  #[arg(short, long)]
  keep: bool,

  /// The branch's stash to push to
  #[arg(short, long)]
  branch: Option<String>,

  /// Stash message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  message: Vec<String>,
}

impl PushArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;
    let head = repo.head()?;

    let branch = match &self.branch {
      Some(name) => BranchMeta::from_name_dwim(repo, name)?
        .with_context(|| format!("Failed to find branch: {}", name))?,
      None => {
        if !head.is_branch() {
          return Err(anyhow!(NOT_ON_BRANCH_MSG));
        }

        BranchMeta::from_reference(&head.resolve()?)?
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

    // build tree of stash changes
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
        .context("Failed to build stash changes")?;

      let tree_id = index.write_tree_to(repo)?;
      repo.find_tree(tree_id)?
    };

    // TODO: add more parents to diff between staged, unstaged, and untracked files
    // in stash commit
    let parents: Vec<Commit> = vec![parent];

    // create stash commit
    let commit_id = repo.commit(
      None,
      &sig,
      &sig,
      &msg,
      &tree,
      // map from Vec<Commit> to Vec<&Commit>
      &parents.iter().collect::<Vec<_>>(),
    )?;

    let stash = repo.find_commit(commit_id)?;

    // create/update reference
    let stash_refname = format!("refs/feature/stashes/{}", branch.name());
    match repo.find_reference(&stash_refname) {
      // a stash ref exists for this branch, update it
      Ok(mut stash_ref) => {
        stash_ref.set_target(commit_id, &msg)?;
      }

      // no stash ref exists for this branch, create it
      Err(e) if e.code() == ErrorCode::NotFound => {
        repo.reference(&stash_refname, commit_id, false, &msg)?;

        // create the stash's reflog (not done automatically)
        let mut reflog = repo.reflog(&stash_refname)?;
        reflog.append(commit_id, &sig, Some(&msg))?;
        reflog.write()?;
      }

      Err(e) => return Err(anyhow!(e)),
    }

    // remove stashed changes from workdir
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
          .context("Stash was created, but changes cannot be removed from working directory")?;

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
      "{} changes on {}",
      style("Stashed").green(),
      style(branch.name()).cyan()
    );

    println!(
      "{}",
      display_commit(&stash, &DisplayCommitOptions {
        time: DisplayTimeOptions {
          relative: false,
          fmt: String::new(),
        },
        message: DisplayCommitMessageLevel::Full,
      },)?
    );

    let parent = stash
      .parent(0)
      .expect("Failed to get first parent of stash commit");
    let mut diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&stash.tree()?), None)?;

    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;
    println!("\n{}", summary);

    Ok(())
  }
}
