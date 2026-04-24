use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{Branch, BranchType, Diff, ObjectType, Oid, Repository, Status, StatusOptions};

use crate::App;
use crate::cli::prune::prune_branches;
use crate::util::branch::{branch_to_name, fetch_all, get_current_branch_name, get_upstream};
use crate::util::diff::DiffSummary;
use crate::util::display::trim_hash;

const LONG_ABOUT: &str = r"Updates all branches with their remotes (if they have one), then prunes merged
feature branches.

Branches are fast-forwarded, meaning they may fail to update if their history is
diverged from upstream. That must be resolved manually.

The currently checked-out branch cannot be updated if there are changes in the
working directory. If so, only the current branch will be skipped.";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Updates branches with their remotes and prunes redundant branches", long_about = LONG_ABOUT)]
pub struct Args {
  /// Display output but don't modify any branches. Will still fetch all remotes.
  #[arg(long)]
  pub dry_run: bool,

  /// Don't prune after updating
  #[arg(short = 'P', long, value_name = "SKIP", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_prune: Option<bool>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    fetch_all(&state.repo)?;

    if self.dry_run {
      println!(
        "{}",
        style("Running in dry-run mode, no branches will be updated or deleted").dim()
      );
    }

    let current_branch =
      get_current_branch_name(&state.repo).context("Failed to get current branch")?;
    let branches = state.repo.branches(Some(BranchType::Local))?;

    for (mut branch, _) in branches.flatten() {
      let branch_name = branch_to_name(&branch)?.to_string();
      let is_current = current_branch.as_ref().is_some_and(|it| *it == branch_name);

      let upstream = get_upstream(&branch)?;
      let Some(upstream) = upstream else {
        // no upstream, nothing to update
        continue;
      };
      let upstream_name = branch_to_name(&upstream)?;

      if is_current {
        // check for local changes
        if has_local_changes(&state.repo)? {
          eprintln!(
            r"Cannot update {} with uncommitted changes. You resolve this by:

1. Stashing the changes
  git stash push -m <message>
  feature sync or git pull

2. Discarding the changes
  git reset --hard <upstream_name>
",
            branch_name
          );
          continue;
        }
      }

      if let Err(e) = fast_forward(
        &state.repo,
        &mut branch,
        &branch_name,
        &upstream,
        &upstream_name,
        is_current,
        self.dry_run,
      ) {
        eprintln!("Failed to update: {}", e);
        continue;
      }
    }

    if !self.no_prune.unwrap_or(!state.config.sync.prune) {
      prune_branches(state, self.dry_run)?;
    }
    Ok(())
  }
}

/// Fast-forwards a branch to match upstream. Set `current` to true when fast-forwarding the
/// currently checked-out branch, so that HEAD and the workdir are correctly updated.
/// # Errors
/// If the branch cannot be fast-forwarded.
fn fast_forward(
  repo: &Repository,
  branch: &mut Branch,
  branch_name: &str,
  upstream: &Branch,
  upstream_name: &str,
  current: bool,
  dry_run: bool,
) -> Result<()> {
  let branch_tip = branch.get().peel_to_commit()?;
  let upstream_tip = upstream.get().peel_to_commit()?;

  // already up to date
  if branch_tip.id() == upstream_tip.id() {
    return Ok(());
  }

  let can_ff = repo.graph_descendant_of(upstream_tip.id(), branch_tip.id())?;

  if !can_ff {
    return Err(anyhow!(
      r"{0} cannot be fast-forwarded. You can resolve this by:

1. Forcing the branches to match:
  git checkout {0}
  git reset --hard {1}

2. Manually merging or rebasing:
  git checkout {0}
  git merge/rebase {1}

Option (1) is recommended for base branches that are supposed to reflect the
remote copy rather than be modified directly (e.g. if you're working on a
project with others, or the branch has branch protections on the remote).
",
      branch_name,
      upstream_name
    ));
  }

  let mut diff = repo.diff_tree_to_tree(
    Some(&branch.get().peel_to_tree()?),
    Some(&upstream.get().peel_to_tree()?),
    None,
  )?;
  diff.find_similar(None)?;

  if dry_run {
    display_update(branch_name, &diff, &branch_tip.id())?;
    return Ok(());
  }

  if current {
    // to update the current branch, we also need to update HEAD. this is just a hard reset
    let obj = repo.find_object(upstream_tip.id(), Some(ObjectType::Commit))?;
    repo.reset(&obj, git2::ResetType::Soft, None)?;
  } else {
    // for other branches, we just move them to the upstream commit
    branch
      .get_mut()
      .set_target(upstream_tip.id(), "feature sync fast-forward")?;
  }

  display_update(branch_name, &diff, &branch_tip.id())?;
  Ok(())
}

/// Whether there are any uncommitted changes
fn has_local_changes(repo: &Repository) -> Result<bool> {
  let mut opts = StatusOptions::new();
  opts.include_untracked(false);

  let statuses = repo
    .statuses(Some(&mut opts))
    .expect("Failed to get repository statuses");

  for entry in &statuses {
    let status = entry.status();

    if status.intersects(
      Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE
        | Status::WT_MODIFIED
        | Status::WT_DELETED
        | Status::WT_RENAMED
        | Status::WT_TYPECHANGE,
    ) {
      // return true immediately if any of the above changes are found
      return Ok(true);
    }
  }

  Ok(false)
}

fn display_update(branch_name: &str, diff: &Diff, old_id: &Oid) -> Result<()> {
  println!(
    "{} {} {} | {}",
    style("Updated").green(),
    branch_name,
    style(format!("(was {})", trim_hash(old_id))).dim(),
    DiffSummary::new(diff)?.display_header()
  );
  Ok(())
}
