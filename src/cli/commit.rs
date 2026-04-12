//! Commit subcommand

use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{Commit, Oid, Repository, Signature};

use crate::util::advice::NO_SIGNATURE_MSG;
use crate::util::branch::get_current_branch_name;
use crate::util::diff::DiffSummary;
use crate::util::display::{display_signature, trim_hash};
use crate::util::{get_current_commit, get_signature};
use crate::{lossy, open_repo};

const AMEND_LONG_HELP: &str = r"Amend the previous commit. Remaining args overwrite the previous commit message.
If no remaining args are specified, the previous commit message is used.";

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Commit staged changes")]
pub struct Args {
  /// Whether to amend the previous commit
  #[arg(long, long_help = AMEND_LONG_HELP)]
  amend: bool,

  /// Bypass precommit hooks
  #[arg(long)]
  no_verify: bool,

  /// Words to join together as commit message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  words: Vec<String>,
}

impl Args {
  pub fn run(&self) -> Result<()> {
    let repo = open_repo!();
    let msg = self.words.join(" ");

    // most recent commit, i.e. commit that HEAD points to. None when repository has no commits
    let current_commit = get_current_commit(&repo)?;
    let signature = get_signature(&repo)?.ok_or(anyhow!(NO_SIGNATURE_MSG))?;
    let mut index = repo.index().context("Failed to get staged changes")?;

    let tree_id = index.write_tree().context("Failed to get index tree")?;
    let tree = repo
      .find_tree(tree_id)
      .context("Failed to get index tree")?;

    // all the info needed for amend
    if self.amend {
      let current_commit = current_commit.ok_or(anyhow!("No commits yet, cannot amend"))?;
      self.pre_commit(&repo)?;

      let new_id = current_commit
        .amend(
          Some("HEAD"),
          None,
          Some(&signature),
          None,
          if !msg.is_empty() { Some(&msg) } else { None },
          Some(&tree),
        )
        .expect("Failed to amend commit");

      self.display_commit(&repo, Some(&current_commit.id()), &new_id, &signature, &msg)?;
      return Ok(());
    }

    let commit_tree = current_commit.as_ref().and_then(|it| it.tree().ok());

    let staged_diff = repo
      .diff_tree_to_index(commit_tree.as_ref(), Some(&index), None)
      .context("Failed to analyze staged changes")?;

    let staged_stats = staged_diff
      .stats()
      .context("Failed to analyze staged changes")?;

    if staged_stats.files_changed() == 0 {
      return Err(anyhow!(
        r#"Nothing to commit! Stage some changes with "git add …""#
      ));
    }

    // not an amend, must specify a message
    if msg.is_empty() {
      return Err(anyhow!("Must specify a commit message"));
    }

    let old_id = current_commit.as_ref().map(|it| it.id());
    let parent_commits: Vec<Commit> = current_commit.into_iter().collect();
    let parent_refs: Vec<&Commit> = parent_commits.iter().collect();

    self.pre_commit(&repo)?;

    let new_id = repo
      .commit(
        Some("HEAD"),
        &signature,
        &signature,
        &msg,
        &tree,
        &parent_refs,
      )
      .expect("Failed to commit");

    self.display_commit(&repo, old_id.as_ref(), &new_id, &signature, &msg)?;
    Ok(())
  }

  fn pre_commit(&self, repo: &Repository) -> Result<()> {
    if self.no_verify {
      println!("{}", style("Skipping precommit hook").yellow());
      let _ = std::io::stdout().flush(); // flush is for ux, but isn't a big deal if it fails
      return Ok(());
    }

    let git_dir = repo.path();
    let script = git_dir.join("hooks").join("pre-commit");

    if !script.exists() {
      // no hooks set, always succeed
      return Ok(());
    }

    print!("Running precommit hook…");
    let _ = std::io::stdout().flush();

    let output = Command::new(script).output()?;

    if output.status.success() {
      println!(" {}", style("passed!").green());
      let _ = std::io::stdout().flush();
      Ok(())
    } else {
      println!(" {}", style("failed!").red());
      let _ = std::io::stdout().flush();
      eprintln!("Precommit output:");
      eprintln!();
      eprintln!("{}", lossy!(&output.stderr));
      Err(anyhow!("Precommit hook failed"))
    }
  }

  /// Prints success output
  ///
  /// Params
  /// - `old_id` - Oid of the parent commit of a regular commit, or the commit that was replaced for
  ///   an amend
  /// - `new_id` - The oid of the newly created commit
  /// - `signature` - The signature used on the commit
  /// - `msg` - The commit message, which may be empty for amends
  /// - `amend` - Whether or not the commit was an amend
  fn display_commit(
    &self,
    repo: &Repository,
    old_id: Option<&Oid>,
    new_id: &Oid,
    signature: &Signature,
    msg: &str,
  ) -> Result<()> {
    // action
    let mut out = format!(
      "{}",
      if self.amend {
        style("Amended").green()
      } else {
        style("Committed").green()
      }
    );

    // hash
    out.push_str(
      &style(format!(
        " {}",
        if self.amend {
          let old = match &old_id {
            Some(it) => &trim_hash(it),
            None => "unknown",
          };
          format!("{} -> {}", old, trim_hash(new_id))
        } else {
          trim_hash(new_id).to_string()
        }
      ))
      .yellow()
      .to_string(),
    );

    // branch
    out.push_str(&format!(" on {}", match get_current_branch_name(repo) {
      Ok(it) => match it {
        Some(name) => style(name).blue().to_string(),
        None => style("unknown").red().to_string(),
      },
      Err(_) => style("unknown").red().to_string(),
    }));

    // signature
    out.push_str(&format!(" as {}", display_signature(Some(signature))));

    out.push('\n');

    // message
    if !msg.is_empty() {
      out.push('\n'); // double space
      out.push_str(msg);
      out.push('\n');
    }

    let new_commit = repo
      .find_commit(*new_id)
      .context("Failed to find reference to new commit")?;
    let new_tree = new_commit.tree().ok();

    let old_commit = old_id.and_then(|it| repo.find_commit(*it).ok());
    let old_tree = old_commit.and_then(|it| it.tree().ok());

    let diff = repo
      .diff_tree_to_tree(old_tree.as_ref(), new_tree.as_ref(), None)
      .context("Failed to obtain commit changes")?;

    let summary = DiffSummary::new(&diff);

    let diff_out = match summary {
      Ok(it) => it.to_string(),
      Err(_) => style("Failed to get commit changes").red().to_string(),
    };

    out.push('\n'); // double space
    out.push_str(&diff_out);
    println!("{}", out);
    Ok(())
  }
}
