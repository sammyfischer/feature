use anyhow::{Result, anyhow};
use console::style;
use git2::{Branch, BranchType, Commit, Diff, Repository};

use crate::App;
use crate::cli::prune::prune_branches;
use crate::util::branch::{fetch_all, get_current_branch_name, hard_reset};
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::{DiffSummary, has_workdir_changes};
use crate::util::display::trim_hash;

const LONG_ABOUT: &str = r"Updates all branches with their remotes (if they have one), then prunes merged
feature branches.

Branches are fast-forwarded, meaning they may fail to update if their history is
diverged from upstream. That must be resolved manually.

The currently checked-out branch cannot be updated if there are changes in the
working directory. If so, only the current branch will be skipped.";

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Updates branches with their remotes and prunes redundant branches",
  long_about = LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
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

    let current_branch = get_current_branch_name(&state.repo)?;
    let branches = state.repo.branches(Some(BranchType::Local))?;

    for (mut branch, _) in branches.flatten() {
      let branch_meta = BranchMeta::from_branch(&branch)?;
      let is_current = current_branch
        .as_ref()
        .is_some_and(|it| *it == branch_meta.name());

      let upstream = branch_meta.upstream(&state.repo)?;
      let Some(upstream) = upstream else {
        // no upstream, nothing to update
        continue;
      };
      let upstream_meta = BranchMeta::from_branch(&upstream)?;

      if is_current {
        // check for local changes
        if has_workdir_changes(&state.repo)? {
          println!(
            "{} {} due to local changes",
            style("Skipping").yellow(),
            branch_meta.name()
          );
          continue;
        }
      }

      if let Err(e) = fast_forward(
        &state.repo,
        &mut branch,
        &branch_meta,
        &upstream,
        &upstream_meta,
        is_current,
        self.dry_run,
      ) {
        println!(
          "{} to update {}: {}",
          style("Failed").red(),
          branch_meta.name(),
          e
        );
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
  branch_meta: &BranchMeta,
  upstream: &Branch,
  upstream_meta: &BranchMeta,
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
      "{} and {} have diverged",
      branch_meta.name(),
      upstream_meta.name()
    ));
  }

  let mut diff = repo.diff_tree_to_tree(
    Some(&branch.get().peel_to_tree()?),
    Some(&upstream.get().peel_to_tree()?),
    None,
  )?;
  diff.find_similar(None)?;

  if dry_run {
    display_update(branch_meta.name(), &diff, &branch_tip)?;
    return Ok(());
  }

  if current {
    // to update the current branch, we also need to update HEAD. this is just a hard reset
    hard_reset(repo, upstream.get())?;
  } else {
    // for other branches, we just move them to the upstream commit
    branch.get_mut().set_target(
      upstream_tip.id(),
      &format!("feature sync: fast-forward to {}", upstream_meta.refname()),
    )?;
  }

  display_update(branch_meta.name(), &diff, &branch_tip)?;
  Ok(())
}

fn display_update(branch_name: &str, diff: &Diff, old_commit: &Commit) -> Result<()> {
  println!(
    "{} {} {} | {}",
    style("Updated").green(),
    branch_name,
    style(format!("(was {})", trim_hash(old_commit)?)).dim(),
    DiffSummary::new(diff)?.display_header()
  );
  Ok(())
}
