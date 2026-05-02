use std::fmt::Write;
use std::fs;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{ErrorCode, FetchOptions, Oid, Rebase, RemoteCallbacks, Repository};

use crate::util::TrimPrefix;
use crate::util::advice::{NO_SIGNATURE_MSG, REBASE_CONFLICT_ADVICE};
use crate::util::branch::get_head;
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::DiffSummary;
use crate::util::display::trim_hash;
use crate::util::{credentials_cb, get_current_commit, resolve_commit_name};
use crate::{App, data, style};

const LONG_ABOUT: &str = r"Rebases this branch onto its base. The available commands are similar to a git
rebase.";

const NO_BASE_MSG: &str = r#"No base branch found. You can:

• Manually specify the base branch: "feature update <BASE_BRANCH>"
• Set the base branch permanently: "feature base <BASE_BRANCH>""#;

const COMMIT_FAILED_MSG: &str = r#"Failed to apply commit. You can:

• Try to continue with "git rebase --continue"
• Skip applying the current commit with "git rebase --skip"
• Abort the rebase with "git rebase --abort""#;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Updates this branch with its base", long_about = LONG_ABOUT)]
pub struct Args {
  /// Output which base branch will be used, but don't perform the rebase
  #[arg(long)]
  dry_run: bool,

  /// Continue an active rebase
  #[arg(short, long)]
  r#continue: bool,

  /// Abort an active rebase
  #[arg(short, long)]
  abort: bool,

  /// The name of the base branch to use.
  #[arg(value_name = "BRANCH-ISH")]
  base: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    if self.r#continue {
      return self.rebase_continue(&state.repo);
    }

    if self.abort {
      return self.rebase_abort(&state.repo);
    }

    // fail if there's already an active rebase
    if self.is_rebase_active(&state.repo)? {
      return Err(anyhow!("A rebase is already in progress"));
    }

    let branch =
      BranchMeta::current(&state.repo)?.context("Not currently on a branch! Nothing to update.")?;

    let base = match &self.base {
      Some(base_name) => BranchMeta::from_name_dwim(&state.repo, base_name)?
        .ok_or(anyhow!("Branch not found: {}", base_name))?,
      None => data::get_feature_base(&state.repo, branch.name())?.ok_or(anyhow!(NO_BASE_MSG))?,
    };

    if self.dry_run {
      println!("Using base: {}", base.name());
      return Ok(());
    }

    // if base is a remote, fetch the latest
    if base.is_remote() {
      let (shorter_name, remote_name) = base.split_name_and_remote()?;
      let mut remote =
        state
          .repo
          .find_remote(&remote_name.unwrap_or_else(|| {
            panic!("Remote should exist on upstream branch: {}", base.name())
          }))?;

      let mut opts = FetchOptions::new();
      let mut cbs = RemoteCallbacks::new();
      cbs.credentials(credentials_cb);
      opts.remote_callbacks(cbs);

      remote.fetch(
        &[&format!("+refs/heads/{}:{}", shorter_name, base.refname())],
        Some(&mut opts),
        None,
      )?;

      println!("{}", style!("Fetched {}", base.name()).dim());
    }

    let base_commit = state
      .repo
      .find_annotated_commit(base.resolve(&state.repo)?.peel_to_commit()?.id())?;

    let mut rebase = state
      .repo
      .rebase(None, Some(&base_commit), None, None)
      .context("Failed to initiate rebase")?;

    self.rebase(&state.repo, &mut rebase)?;
    Ok(())
  }

  /// Runs the given rebase until it finishes or encounters a conflict
  fn rebase(&self, repo: &Repository, rebase: &mut Rebase) -> Result<()> {
    while let Some(op) = rebase.next() {
      let old_id = op.context("Failed to get next rebase operation")?.id();
      let old_commit = repo.find_commit(old_id)?;

      let index = repo
        .index()
        .context("Failed to get index to build rebase commit on")?;

      if index.has_conflicts() {
        let commit = get_current_commit(repo)?;
        match commit {
          Some(commit) => {
            let tree = commit.tree()?;
            let diff = repo.diff_tree_to_index(Some(&tree), Some(&index), None)?;
            let summary = DiffSummary::new(&diff)?;

            eprintln!("{}", REBASE_CONFLICT_ADVICE);

            println!(
              "\n{} - {}",
              style("Conflicts").yellow(),
              if summary.num_files != 0 {
                summary.display_conflicts()
              } else {
                style("none").green().to_string()
              }
            );
          }
          None => println!("Failed to display conflicts"),
        }
        self.dump_rebase(repo, rebase)?;
        return Err(anyhow!("Rebase conflicts"));
      }

      let signature = repo.signature().context(NO_SIGNATURE_MSG)?;

      let new_id = rebase
        .commit(None, &signature, None)
        .context(COMMIT_FAILED_MSG)?;

      let new_commit = repo.find_commit(new_id)?;

      println!(
        "{} commit {} as {}",
        style("Applied").green(),
        style(trim_hash(&old_commit)?).blue(),
        style(trim_hash(&new_commit)?).magenta()
      );
    }

    let orig_head = rebase.orig_head_id();
    let curr_head = get_head(repo)?;

    let summary = if let Some(orig_head) = orig_head
      && let Some(curr_head) = curr_head
    {
      let old = repo.find_commit(orig_head)?.tree()?;
      let new = curr_head.peel_to_tree()?;
      let mut diff = repo.diff_tree_to_tree(Some(&old), Some(&new), None)?;
      diff.find_similar(None)?;
      Some(DiffSummary::new(&diff)?)
    } else {
      None
    };

    let out = display_success(repo, rebase, summary.as_ref())?;
    rebase.finish(None).context("Failed to finish rebase")?;
    println!("{}", out);
    Ok(())
  }

  /// Opens and continues an existing rebase until it finishes or there's a conflict
  fn rebase_continue(&self, repo: &Repository) -> Result<()> {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would continue it")
      } else {
        println!("There is no active rebase, this command would fail")
      }
      return Ok(());
    }

    let mut rebase = repo.open_rebase(None).context("Failed to open rebase")?;
    self.rebase(repo, &mut rebase)?;
    Ok(())
  }

  /// Opens and aborts an existing rebase
  fn rebase_abort(&self, repo: &Repository) -> Result<()> {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would abort it")
      } else {
        println!("There is no active rebase, this command would fail")
      }
      return Ok(());
    }

    let mut rebase = repo.open_rebase(None).context("Failed to open rebase")?;
    rebase.abort().context("Failed to abort rebase")?;
    println!("{} rebase", style("Aborted").green());
    Ok(())
  }

  /// Whether a rebase is currently active. Panics if there's an unknown error
  fn is_rebase_active(&self, repo: &Repository) -> Result<bool> {
    match repo.open_rebase(None) {
      Ok(_) => Ok(true),
      Err(e) if e.code() == ErrorCode::NotFound => Ok(false),
      Err(e) => panic!("Failed to check for active rebase: {}", e),
    }
  }

  /// Dumps remaining rebase steps into the git-rebase-todo. Allows the user to use native git
  /// rebase commands.
  fn dump_rebase(&self, repo: &Repository, rebase: &mut Rebase) -> Result<()> {
    let current = rebase
      .operation_current()
      .expect("There should be a current rebase operation");

    let total = rebase.len();
    // always 40 char hash, some extra space for the operation. there will always be at least one
    // line
    let mut buf = String::with_capacity(50);

    for i in (current + 1)..total {
      let op = rebase
        .nth(i)
        .unwrap_or_else(|| panic!("Failed to find rebase operation number {}", i));

      // commit id
      let id = op.id();

      // rebase operations are pick by default
      writeln!(buf, "pick {}", id)
        .unwrap_or_else(|_| panic!("Failed to write rebase operation {}", i));
    }

    let rebase_data_dir = repo.path().join("rebase-merge");

    // git uses the git-rebase-todo file to continue an existing rebase
    let path = rebase_data_dir.join("git-rebase-todo");
    fs::write(&path, &buf).context("Failed to write remaining operations to file")?;

    // libgit2 uses a file called current which just stores the current oid
    let id = rebase
      .nth(current)
      .expect("There should be a current rebase operation")
      .id()
      .to_string();

    fs::write(rebase_data_dir.join("current"), id)
      .context("Failed to write current rebase operation to file")?;

    Ok(())
  }
}

/// Display a message upon successful rebase
fn display_success(
  repo: &Repository,
  rebase: &Rebase,
  summary: Option<&DiffSummary>,
) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::with_capacity(100);

  let branch_name = match rebase.orig_head_name() {
    Some(name) => style(
      name
        .trim_prefix_opt("refs/remotes/")
        .trim_prefix_opt("refs/heads/"),
    )
    .blue(),
    None => style("unknown").red(),
  };

  let base_name = fs::read_to_string(repo.path().join("rebase-merge/onto"))?;
  let base_name = base_name.trim();
  let base_commit = repo.find_commit(Oid::from_str(base_name)?)?;
  let base_name = resolve_commit_name(repo, &base_commit)?;

  write!(
    out,
    "{} {} with changes from {}",
    style("Updated").green(),
    branch_name,
    style(base_name).magenta()
  )?;

  if let Some(summary) = summary {
    write!(out, "\n{}", summary)?;
  }

  Ok(out)
}
