use std::fs;
#[cfg(unix)]
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use console::style;
use git2::{Oid, Repository, Signature};

use crate::App;
use crate::cli::advice::NOT_ON_BRANCH_MSG;
use crate::cli::commit::get_editor;
use crate::cli::display::commit::{DisplayCommitOptions, display_commit};
use crate::cli::display::diff::display_summary_with_header;
use crate::cli::display::time::DisplayTimeOptions;
use crate::core::branch::{get_head_resolved, switch};
use crate::core::branch_info::BranchInfo;
use crate::core::diff::DiffSummary;
use crate::core::user_config::{CommitMessageLevel, UserConfig};

const LONG_ABOUT: &str = r#"Squashes all commits on a feature branch into a single commit.

This commit is placed at the merge-base of the branch and its base (the result of
"git merge-base <branch> <base>").

A squash can be undone in two ways:
• "git reset --hard @{1}" to revert the current branch
• "git branch -f <branch> <branch>@{1}" to reset a different branch"#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Squashes branch into a single commit",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct SquashArgs {
  /// Invoke an editor to modify the generated squash message
  #[arg(short, long)]
  edit: bool,

  /// The branch to treat as the base
  #[arg(long, value_name = "BRANCH", value_hint = ValueHint::Other)]
  base: Option<String>,

  /// The branch to squash
  #[arg(long, value_hint = ValueHint::Other)]
  branch: Option<String>,

  /// The subject-line of the squash message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  message: Vec<String>,
}

impl SquashArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;
    let config = UserConfig::new(repo)?;

    let (branch, mut branch_ref) = match self.branch.as_ref() {
      Some(name) => {
        let branch = BranchInfo::from_name_dwim(repo, name)?
          .with_context(|| format!("Failed to find branch matching name: {}", name))?;

        let branch_ref = branch.resolve(repo)?;

        (branch, branch_ref)
      }

      None => {
        let branch_ref = get_head_resolved(repo)?.context(NOT_ON_BRANCH_MSG)?;
        if !branch_ref.is_branch() {
          return Err(anyhow!(NOT_ON_BRANCH_MSG));
        }

        let branch = BranchInfo::from_reference(&branch_ref)?;

        (branch, branch_ref)
      }
    };

    let branch_commit = branch_ref.peel_to_commit()?;
    let branch_tree = branch_commit.tree()?;

    let (base, base_ref) = match self.base.as_ref() {
      Some(name) => {
        let base = BranchInfo::from_name_dwim(repo, name)?
          .with_context(|| format!("Failed to find branch matching name: {}", name))?;

        let base_ref = base.resolve(repo)?;

        (base, base_ref)
      }

      None => {
        let base = config
          .branch_base(branch.name())?
          .with_context(|| format!("Branch {} does not have a base", branch.name()))?;

        let base_ref = base.resolve(repo)?;

        (base, base_ref)
      }
    };

    let base_commit = base_ref.peel_to_commit()?;

    // the common ancestor between branch and base
    let merge_base = {
      let id = repo.merge_base(branch_commit.id(), base_commit.id())?;
      repo.find_commit(id)?
    };

    let sig = repo.signature()?;

    let mut msg = build_message(
      repo,
      if !self.message.is_empty() {
        self.message.join(" ")
      } else {
        format!("Squash {} onto {}", branch.name(), base.name())
      },
      branch_commit.id(),
      merge_base.id(),
      &sig,
    )?;

    if self.edit {
      // prefill SQUASH_EDITMSG
      let path = repo.path().join("SQUASH_EDITMSG");
      fs::write(&path, &msg)?;

      // allow user to edit the file
      invoke_editor(repo)?;

      // re-read message
      msg = fs::read_to_string(&path)?;
    }

    let squash_id = repo.commit(None, &sig, &sig, &msg, &branch_tree, &[&merge_base])?;
    let squash = repo.find_commit(squash_id)?;

    // update ref
    branch_ref.set_target(
      squash_id,
      &format!("squash: collapse branch onto {}", base.name()),
    )?;

    // checkout if current branch was updated
    if let Some(head) = get_head_resolved(repo)?
      && head.name()? == branch.refname()
    {
      switch(repo, &branch)?;
    }

    let (ahead, _) = repo.graph_ahead_behind(branch_commit.id(), merge_base.id())?;

    println!(
      "{} {} commits from {} since {}",
      style("Squashed").green(),
      style(ahead).cyan(),
      style(branch.name()).blue(),
      style(base.name()).magenta()
    );

    println!(
      "{}",
      display_commit(&squash, &DisplayCommitOptions {
        time: DisplayTimeOptions {
          relative: false,
          fmt: config.format_date()?,
        },
        message: CommitMessageLevel::Subject,
      },)?,
    );

    let mut diff =
      repo.diff_tree_to_tree(Some(&merge_base.tree()?), Some(&squash.tree()?), None)?;
    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;
    println!(
      "\nSquashed changes - {}",
      display_summary_with_header(
        &summary,
        &format!(
          "{} {}",
          style(summary.num_files).cyan(),
          if summary.num_files == 1 {
            "file"
          } else {
            "files"
          }
        ),
        config.nerdfont()?
      )
    );

    Ok(())
  }
}

/// Builds the default squash message.
///
/// # Params
/// - `subject` - The subject line to use for the commit message
/// - `start` - The commit to start graph traversal from. This commit's message
///   will be included in the generated message.
/// - `until` - The commit to end traversal. This should be reachable from
///   `start`. This commit's message will not be included in the generated
///   message.
/// - `sig` - The signature used to author the squash commit. If any commits
///   have a different author, they will be included in a co-author footer.
fn build_message(
  repo: &Repository,
  subject: String,
  start: Oid,
  until: Oid,
  sig: &Signature,
) -> Result<String> {
  let mut msgs = Vec::new();
  msgs.push(subject);

  let mut co_authors = Vec::new();

  let mut walk = repo.revwalk()?;
  walk.push(start)?;

  for id in walk {
    let id = id?;
    if id == until {
      break;
    }

    let c = repo.find_commit(id)?;
    let author = c.author();

    if author.name_bytes() != sig.name_bytes() || author.email_bytes() != sig.email_bytes() {
      co_authors.push(format!(
        "Co-authored-by: {} <{}>",
        author.name()?,
        author.email()?
      ));
    }

    msgs.push(format!("* {}", c.message()?.trim()));
  }

  if !co_authors.is_empty() {
    msgs.push(co_authors.join("\n"));
  }

  Ok(msgs.join("\n\n"))
}

/// Invokes and waits for the editor. The resulting message is left in
/// SQUASH_EDITMSG.
fn invoke_editor(repo: &Repository) -> Result<()> {
  let editor = get_editor(repo)?;
  let editmsg = repo.path().join("SQUASH_EDITMSG");

  // run the editor in a shell to parse args
  #[cfg(unix)]
  let status = Command::new("sh")
    .arg("-c")
    .arg(format!("{} \"{}\"", editor, &editmsg.to_string_lossy()))
    .status()?;

  #[cfg(windows)]
  let status = Command::new("cmd")
    .args([
      "/C",
      &format!("{} \"{}\"", editor, &editmsg.to_string_lossy()),
    ])
    .status()?;

  if !status.success() {
    return Err(anyhow!("Editor failed with status: {}", status));
  }

  Ok(())
}
