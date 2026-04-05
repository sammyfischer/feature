//! Commit subcommand

use std::process::Command;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{Commit, Delta, Oid, Repository, Signature};

use crate::cli::{get_current_branch, get_current_commit};
use crate::open_repo;

macro_rules! delta_filename {
  ($delta:ident, $file:ident) => {
    $delta
      .$file()
      .path()
      .expect("Failed to get file path from delta")
      .display()
      .to_string()
  };
}

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

    let signature = repo
      .signature()
      .expect("Failed to get default commit signature");

    let mut index = repo.index().expect("Failed to get current index");
    let tree_id = index.write_tree().expect("Failed to get index tree id");
    let tree = repo.find_tree(tree_id).expect("Failed to get index tree");

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

      self.print_output(&repo, Some(&current_commit.id()), &new_id, &signature, &msg)?;
      return Ok(());
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

    self.print_output(&repo, old_id.as_ref(), &new_id, &signature, &msg)?;
    Ok(())
  }

  fn pre_commit(&self, repo: &Repository) -> Result<()> {
    if self.no_verify {
      return Ok(());
    }

    let git_dir = repo.path();
    let script = git_dir.join("hooks").join("pre-commit");

    if !script.exists() {
      // no hooks set, always succeed
      return Ok(());
    }

    let output = Command::new(script).output()?;

    if output.status.success() {
      Ok(())
    } else {
      eprintln!("Precommit output:");
      eprintln!();
      eprintln!("{}", String::from_utf8_lossy(&output.stderr));
      Err(anyhow!("Precommit hooks failed"))
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
  fn print_output(
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
            Some(it) => &it.to_string()[..7],
            None => "unknown",
          };
          format!("{} -> {}", old, &new_id.to_string()[..7])
        } else {
          new_id.to_string()[..7].to_string()
        }
      ))
      .yellow()
      .to_string(),
    );

    // branch
    out.push_str(&format!(" on {}", match get_current_branch(repo) {
      Ok(it) => style(it).blue().to_string(),
      Err(_) => style("unknown").red().to_string(),
    }));

    // signature
    out.push_str(&format!(
      " as {} {}",
      style(
        signature
          .name()
          .expect("Default signature name should be valid utf-8")
      )
      .cyan(),
      style(
        signature
          .email()
          .expect("Default signature email should be valid utf-8")
      )
      .dim()
    ));

    out.push('\n');

    // message
    if !msg.is_empty() {
      out.push('\n'); // double space
      out.push_str(msg);
      out.push('\n');
    }

    let diff_out = match diff_output(repo, new_id, old_id) {
      Ok(it) => it,
      Err(_) => style("Failed to get commit changes").red().to_string(),
    };

    out.push('\n'); // double space
    out.push_str(&diff_out);
    print!("{}", out);
    Ok(())
  }
}

/// Gets a diff between the given commit and its parent and builds a pretty output
fn diff_output(repo: &Repository, new_id: &Oid, old_id: Option<&Oid>) -> Result<String> {
  let mut out = String::new();

  let new_commit = repo
    .find_commit(*new_id)
    .context("Failed to find reference to new commit")?;
  let new_tree = new_commit.tree().ok();

  let old_commit = old_id.and_then(|it| repo.find_commit(*it).ok());
  let old_tree = old_commit.and_then(|it| it.tree().ok());

  let diff = repo
    .diff_tree_to_tree(old_tree.as_ref(), new_tree.as_ref(), None)
    .context("Failed to obtain commit changes")?;

  let stats = diff.stats().context("Failed to get diff stats")?;

  // summary
  out.push_str(&format!(
    "{} files changed [{} {}]\n",
    style(stats.files_changed()).cyan(),
    style(format!("+{}", stats.insertions())).green(),
    style(format!("-{}", stats.deletions())).red()
  ));

  // per-file info
  struct FileChanges {
    status: String,
    name: String,
    insertions: usize,
    deletions: usize,
  }
  let mut files: Vec<FileChanges> = Vec::new();
  // we need a mutable pointer to access `files` in multiple callbacks, but since these callbacks
  // are synchronous it's fine
  let files_ptr: *mut Vec<FileChanges> = &mut files;

  diff.foreach(
    &mut |delta, _| {
      let (status, name) = match delta.status() {
        Delta::Added => (style("A").green(), delta_filename!(delta, new_file)),
        Delta::Deleted => (style("D").red(), delta_filename!(delta, old_file)),
        Delta::Modified => (style("M").yellow(), delta_filename!(delta, new_file)),
        Delta::Renamed => (
          style("R").cyan(),
          format!(
            "{} -> {}",
            delta_filename!(delta, old_file),
            delta_filename!(delta, new_file),
          ),
        ),
        Delta::Copied => (
          style("C").green(),
          format!(
            "{} -> {}",
            delta_filename!(delta, old_file),
            delta_filename!(delta, new_file),
          ),
        ),
        _ => (style("?").dim(), delta_filename!(delta, new_file)),
      };
      unsafe { &mut *files_ptr }.push(FileChanges {
        status: status.to_string(),
        name,
        insertions: 0,
        deletions: 0,
      });
      true
    },
    None,
    None,
    Some(&mut |_, _, line| {
      if let Some(file) = unsafe { &mut *files_ptr }.last_mut() {
        match line.origin_value() {
          git2::DiffLineType::Addition => file.insertions += 1,
          git2::DiffLineType::Deletion => file.deletions += 1,
          _ => {}
        }
      }
      true
    }),
  )?;

  for file in &files {
    out.push_str(&format!(
      "  {} {} {} {}\n",
      file.status,
      file.name,
      style(format!("+{}", file.insertions)).green(),
      style(format!("-{}", file.deletions)).red()
    ));
  }

  Ok(out)
}
