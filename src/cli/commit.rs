//! Commit subcommand

use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{Commit, Oid, Repository, Signature};

use crate::cli::Cli;
use crate::util::advice::NO_SIGNATURE_MSG;
use crate::util::branch::{
  get_current_branch_name,
  get_merge_head,
  get_pick_head,
  get_revert_head,
};
use crate::util::diff::DiffSummary;
use crate::util::display::{display_signature, trim_hash};
use crate::util::term::get_user_confirmation;
use crate::util::{get_current_commit, get_signature, read_commit_msg};
use crate::{lossy, open_repo};

const AMEND_LONG_HELP: &str = r"Amend the previous commit. Remaining args overwrite the previous commit message.
If no remaining args are specified, the previous commit message is used.";

const CONFIRM_DURING_PICK: &str = r#"
There is currently a cherry-pick active. Cherry-picks are finished by resolving
the conflicts and running "git cherry-pick --continue", rather than committing.

Do you want to commit anyway?"#;

const CONFIRM_DURING_REVERT: &str = r#"
There is currently a revert active. Reverts are finished by resolving the
conflicts and running "git revert --continue", rather than committing.

Do you want to commit anyway?"#;

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
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();
    let mut msg = self.words.join(" ");

    // if there's a pick active and the user has pick advice enabled
    if get_pick_head(&repo)?.is_some() && cli.config.advice.cherry_pick {
      let confirmed = get_user_confirmation(CONFIRM_DURING_PICK)?;
      if !confirmed {
        println!("Cancelled commit");
        return Ok(());
      }
    }

    // if there's a revert active and the user has revert advice enabled
    if get_revert_head(&repo)?.is_some() && cli.config.advice.revert {
      let confirmed = get_user_confirmation(CONFIRM_DURING_REVERT)?;
      if !confirmed {
        println!("Cancelled commit");
        return Ok(());
      }
    }

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

    let merge_head = get_merge_head(&repo)?;

    if msg.is_empty() {
      // if it's a merge, try to get the msg from .git/MERGE_MSG
      'merge_msg: {
        if merge_head.is_some() {
          let path = repo.path().join("MERGE_MSG");

          // if not found, default
          if path.exists() {
            let merge_msg = read_commit_msg(&path)
              .context("Failed to get default merge message. Try specifying a message manually.")?;

            // if not empty, use it
            if !merge_msg.is_empty() {
              msg = merge_msg.to_string();
              // break to avoid error since we found the message
              break 'merge_msg;
            }
          }
        }

        // if we didn't break, fall through and error
        return Err(anyhow!("Must specify a commit message"));
      }
    }

    let old_id = current_commit.as_ref().map(|it| it.id());
    let mut parent_commits: Vec<Commit> = current_commit.into_iter().collect();

    // if there's an ongoing merge, the merge head should be the second parent
    if let Some(merge_head) = &merge_head {
      parent_commits.push(merge_head.peel_to_commit()?);
    }

    // get each element as a reference
    let parent_commits: Vec<&Commit> = parent_commits.iter().collect();

    self.pre_commit(&repo)?;

    let new_id = repo
      .commit(
        Some("HEAD"),
        &signature,
        &signature,
        &msg,
        &tree,
        &parent_commits,
      )
      .expect("Failed to commit");

    self.display_commit(&repo, old_id.as_ref(), &new_id, &signature, &msg)?;

    // committing during an active merge completes the merge, we should clean up the merge files
    if merge_head.is_some() {
      repo.cleanup_state()?;
      println!("\n{}", style("Merge completed!").dim())
    }

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
    out.push_str(&format!(" to {}", match get_current_branch_name(repo) {
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
