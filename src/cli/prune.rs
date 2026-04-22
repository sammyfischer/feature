use anyhow::{Context, Result};
use console::style;
use git2::{Branch, BranchType, ErrorCode, Oid, Repository};

use crate::util::branch::{fetch_all, get_current_branch_name};
use crate::util::display::trim_hash;
use crate::{App, await_child, data, git, lossy};

const LONG_ABOUT: &str = r"Deletes all branches that:
• have a known base branch
• are an ancestor of their base branch
• aren't a base or protected branch
• aren't the current branch

These checks should prevent most accidental deletions, and at least ensure that
any deleted branches were redundant (being an ancestor of the base means the
base contains the branch's commit history already).";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Deletes merged feature branches", long_about = LONG_ABOUT)]
pub struct Args {
  /// Display output but don't delete any branches. Will still fetch all remotes.
  #[arg(long)]
  dry_run: bool,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    fetch_all(&state.repo)?;

    if self.dry_run {
      println!(
        "{}",
        style("Running in dry-run mode, nothing will be deleted").dim()
      );
    }

    prune_branches(state, self.dry_run)
  }
}

pub fn prune_branches(state: &App, dry_run: bool) -> Result<()> {
  let branches = state.repo.branches(Some(BranchType::Local))?;
  let current_name = get_current_branch_name(&state.repo)?;

  for (mut branch, _) in branches.flatten() {
    if let Err(e) = safe_delete_branch(state, &mut branch, current_name.as_deref(), dry_run) {
      eprintln!("{}", e);
    }
  }

  Ok(())
}

/// Deletes a branch if:
/// - it's not a base branch
/// - it's not a protected branch
/// - it's not the current branch
/// - it's changes are merged into its base
///
/// # Returns
/// Whether the delete operation occured. `false` means the delete didn't occur because the branch
/// was determined to be unsafe to delete, rather than anything going wrong. An error implies that
/// something went wrong.
fn safe_delete_branch(
  state: &App,
  branch: &mut Branch,
  current_branch_name: Option<&str>,
  dry_run: bool,
) -> Result<bool> {
  let branch_name = lossy!(branch.name_bytes()?).to_string();
  let branch_refname = lossy!(branch.get().name_bytes()).to_string();

  // skip branches that have never been pushed
  match state.repo.branch_upstream_name(&branch_refname) {
    Ok(_) => {}
    Err(e) if e.code() == ErrorCode::NotFound => return Ok(false),
    Err(e) => return Err(e.into()),
  };

  // skip protected branches
  if state.config.protect.contains(&branch_name) {
    return Ok(false);
  }

  // skip current branch
  if current_branch_name.is_some_and(|it| it == branch_name) {
    // not necessarily an error, but the user should know that a non-protected branch was
    // skipped and may manually need to be deleted
    println!(
      "{}",
      style(format!(
        "Skipping currently checked-out branch: {}",
        branch_name
      ))
      .dim()
    );
    return Ok(false);
  }

  // find base branch from db, else skip
  let base = match data::get_feature_base(&state.repo, &branch_name)? {
    Some(base) => base,
    None => return Ok(false),
  };

  let base_name = lossy!(base.name_bytes()?);

  // detect if branch is merged (i.e. has no commits that aren't on its base)
  let is_merged = is_merged(&state.repo, branch, &base).with_context(|| {
    format!(
      "Failed to determine if {} is merged into {}",
      branch_name, base_name
    )
  })?;

  if is_merged {
    let commit = branch
      .get()
      .peel_to_commit()
      .with_context(|| format!("Failed to get commit pointed to by {}", branch_name))?;

    // in dry-run mode, display output but don't delete
    if dry_run {
      display_deletion(&branch_name, &commit.id());
      // still return true, this would've been a deletion
      return Ok(true);
    }

    branch
      .delete()
      .with_context(|| format!("Failed to delete branch {}", branch_name))?;

    display_deletion(&branch_name, &commit.id());

    // git2 can't remove entire config sections, but git provides a command to do so
    let key = format!("branch.{}", &branch_name);
    match git!("config", "--remove-section", key).spawn() {
      Ok(mut cmd) => await_child!(cmd, "Git"),
      Err(e) => Err(e.into()),
    }.with_context(|| format!(
        r#"Failed to delete branch config. Run "git config --remove-section branch.{}" to remove it.""#,
        &branch_name
      ))?;
  }

  Ok(true)
}

/// Whether branch is merged into base. A branch is considered merged if:
/// - it points to the same commit as its base
/// - it's not a descendant of base (i.e. there are no new commits)
fn is_merged(repo: &Repository, branch: &Branch, base: &Branch) -> Result<bool> {
  let branch_commit = branch.get().peel_to_commit()?.id();
  let base_commit = base.get().peel_to_commit()?.id();

  if branch_commit == base_commit {
    return Ok(true);
  }

  // whether branch is a descendant of base. if it is, then there are newer unmerged commits
  let is_descendant = repo.graph_descendant_of(branch_commit, base_commit)?;
  Ok(!is_descendant)
}

fn display_deletion(branch_name: &str, commit_id: &Oid) {
  println!(
    "{} {} {}",
    style("Deleted").red(),
    branch_name,
    &style(&format!("(was {})", &trim_hash(commit_id))).dim()
  );
}
