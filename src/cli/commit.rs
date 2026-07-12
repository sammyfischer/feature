//! Commit subcommand

use std::env::{self, VarError};
use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use console::{strip_ansi_codes, style};
use git2::{Commit, Diff, ErrorCode, MergeOptions, Oid, Reference, Repository, Tree};

use crate::App;
use crate::cli::display::commit::{DisplayCommitOptions, display_commit};
use crate::cli::display::diff::display_summary;
use crate::cli::display::display_hash;
use crate::cli::display::time::DisplayTimeOptions;
use crate::core::advice::NO_SIGNATURE_MSG;
use crate::core::branch::{
  get_current_branch_name,
  get_head,
  get_merge_head,
  get_pick_head,
  get_revert_head,
};
use crate::core::commit::resolve_commit_name;
use crate::core::diff::DiffSummary;
use crate::core::get_signature;
use crate::core::status::has_index_changes;
use crate::core::string::{ToStrLossy, ToStrLossyOwned};
use crate::core::term::get_user_confirmation;
use crate::core::user_config::{CommitMessageLevel, UserConfig};

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
#[command(about = "Commit staged changes", disable_help_subcommand = true)]
pub struct Args {
  /// Where to apply the commit
  #[arg(long, value_name = "BRANCH", value_hint = ValueHint::Other)]
  to: Option<String>,

  /// Invoke the commit message editor
  #[arg(short, long)]
  edit: bool,

  /// Amend the previous commit
  #[arg(long, long_help = AMEND_LONG_HELP)]
  amend: bool,

  /// Change the commit author to yourself
  #[arg(long, requires = "amend")]
  reset_author: bool,

  /// Bypass precommit hooks
  #[arg(long, value_name = "BYPASS")]
  no_verify: bool,

  /// Words to join together as commit message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_hint = ValueHint::Other)]
  words: Vec<String>,
}

struct CommitTarget<'repo> {
  commit: Commit<'repo>,

  /// Something user-friendly to print (ideally branch name, maybe tag or short
  /// hash)
  display_name: String,

  /// The ref to update. Will be None if we're not committing to a branch
  refname: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum CommitType {
  Normal,
  Merge,
  Amend(Oid),
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let config = UserConfig::new(&state.repo)?;

    // if there's a pick active and the user has pick advice enabled
    if get_pick_head(&state.repo)?.is_some() && config.advice_conflict()? {
      let confirmed = get_user_confirmation(CONFIRM_DURING_PICK)?;
      if !confirmed {
        println!("Cancelled commit");
        return Ok(());
      }
    }

    // if there's a revert active and the user has revert advice enabled
    if get_revert_head(&state.repo)?.is_some() && config.advice_conflict()? {
      let confirmed = get_user_confirmation(CONFIRM_DURING_REVERT)?;
      if !confirmed {
        println!("Cancelled commit");
        return Ok(());
      }
    }

    let target = match &self.to {
      Some(to) => {
        let reference = state.repo.resolve_reference_from_short_name(to)?;

        Some(CommitTarget {
          commit: reference.peel_to_commit()?,
          display_name: reference.shorthand_bytes().to_str_lossy_owned(),
          refname: Some(reference.name_bytes().to_str_lossy_owned()),
        })
      }

      None => match get_head(&state.repo)? {
        Some(head) => Some(CommitTarget {
          commit: head.peel_to_commit()?,
          display_name: head.shorthand_bytes().to_str_lossy_owned(),
          refname: Some("HEAD".to_string()),
        }),

        None => None,
      },
    };

    let commit_type = if self.amend {
      let target = target
        .as_ref()
        .ok_or(anyhow!("No current commit to amend"))?;
      CommitType::Amend(target.commit.id())
    } else if state.repo.path().join("MERGE_HEAD").exists() {
      CommitType::Merge
    } else {
      CommitType::Normal
    };

    if commit_type == CommitType::Normal {
      // if it's a normal commit, require non-empty changes
      if !has_index_changes(&state.repo)? {
        return Err(anyhow!(
          "Nothing to commit! Stage some changes with \"git add/rm …\""
        ));
      }
    }

    self.pre_commit_hook(&state.repo)?;

    let (tree, diff) = self.get_changes(&state.repo, target.as_ref())?;
    let sig = get_signature(&state.repo)?.ok_or(anyhow!(NO_SIGNATURE_MSG))?;

    let cli_msg = self.words.join(" ");
    let msg_source = self.get_msg_source(&state.repo, commit_type)?;

    // get initial message
    let default_msg = match msg_source.ty {
      MsgSourceType::Message => cli_msg.as_bytes().to_owned(),

      // MsgSourceType::Template |
      MsgSourceType::Merge | MsgSourceType::Squash => {
        fs::read(state.repo.path().join(&msg_source.file_name))?
      }

      MsgSourceType::Commit(id) => {
        let commit = state.repo.find_commit(id)?;
        commit.message_bytes().to_owned()
      }

      MsgSourceType::None => {
        vec![b'\n']
      }
    };

    // append status info
    let template = build_msg_template(
      &default_msg,
      target.as_ref().map(|target| target.display_name.as_str()),
      &diff,
    )?;

    // populate COMMIT_EDITMSG
    let editmsg = state.repo.path().join("COMMIT_EDITMSG");
    fs::write(&editmsg, template)?;

    // pre-process msg
    self.prepare_msg_hook(&state.repo, &msg_source)?;

    // allow editing
    if self.edit {
      self.invoke_editor(&state.repo)?;
    };

    // post-process commit msg
    self.commit_msg_hook(&state.repo)?;

    // get final message
    let msg = self.read_commit_msg(&editmsg)?;
    let msg = msg.trim();
    if msg.is_empty() {
      return Err(anyhow!("Must specify a commit message!"));
    }

    let mut parents: Vec<Commit> = target
      .as_ref()
      .map(|target| target.commit.clone())
      .into_iter()
      .collect();

    if !self.amend
      && let Some(merge_head) = get_merge_head(&state.repo)?
    {
      parents.push(merge_head.peel_to_commit()?);
    }

    let parents: Vec<&Commit> = parents.iter().collect();

    if self.amend {
      let target = target.ok_or(anyhow!("No current commit to amend"))?;

      // amend the commit
      let new_id = target
        .commit
        .amend(
          target.refname.as_deref(),
          if self.reset_author { Some(&sig) } else { None },
          Some(&sig),
          None,
          Some(msg),
          Some(&tree),
        )
        .context("Failed to amend commit")?;

      println!(
        "{}",
        display_amend_header(&target.commit, &target.display_name)?
      );

      let new_commit = state.repo.find_commit(new_id)?;
      println!("{}", display_commit_details(&new_commit, &diff, &config)?);

      self.post_commit_hook(&state.repo)?;
      self.post_rewrite_hook(&state.repo, target.commit.id(), new_id)?;
      return Ok(());
    }

    let new_id = state
      .repo
      .commit(
        match &target {
          Some(target) => target.refname.as_deref(),
          // empty repo, just update head
          None => Some("HEAD"),
        },
        &sig,
        &sig,
        msg,
        &tree,
        &parents,
      )
      .context("Failed to commit")?;

    let merge_head = get_merge_head(&state.repo)?;
    if let Some(merge_head) = &merge_head {
      println!(
        "{}",
        display_merge_header(
          &state.repo,
          merge_head,
          target.as_ref().map(|it| it.display_name.as_str())
        )?
      );
    } else {
      let target_name = match &target {
        Some(target) => Some(target.display_name.clone()),
        None => get_current_branch_name(&state.repo)?,
      };
      println!("{}", display_commit_header(target_name.as_deref())?);
    };

    let new_commit = state.repo.find_commit(new_id)?;

    println!("{}", display_commit_details(&new_commit, &diff, &config)?);

    // committing during an active merge completes the merge, we should clean up the
    // merge files
    if merge_head.is_some() {
      state.repo.cleanup_state()?;
    }

    self.post_commit_hook(&state.repo)?;
    Ok(())
  }

  /// Computes the changes to commit.
  ///
  /// # Returns
  /// A tuple containing the tree to apply and a diff of the changes
  fn get_changes<'repo>(
    &self,
    repo: &'repo Repository,
    target: Option<&CommitTarget>,
  ) -> Result<(Tree<'repo>, Diff<'repo>)> {
    let Some(target) = target else {
      // simple case, committing to HEAD
      let mut index = repo.index()?;
      let tree_id = index.write_tree()?;
      let tree = repo.find_tree(tree_id)?;

      let head_tree = match get_head(repo)? {
        Some(head) => Some(head.peel_to_tree()?),
        None => None,
      };

      let mut diff = repo.diff_tree_to_tree(head_tree.as_ref(), Some(&tree), None)?;
      diff.find_similar(None)?;
      return Ok((tree, diff));
    };

    // committing to another branch, compute changes with a merge
    let head = get_head(repo)?
      .context("Can't commit to a different branch when there are no commits yet!")?;
    let mut stage = repo.index()?;

    let head_tree = head.peel_to_tree()?;
    let target_tree = target.commit.tree()?;
    let staged_tree = {
      let id = stage.write_tree()?;
      repo.find_tree(id)?
    };

    let mut opts = MergeOptions::new();
    // favor staged changes
    opts.file_favor(git2::FileFavor::Theirs);
    let mut index = repo.merge_trees(&head_tree, &target_tree, &staged_tree, None)?;

    if index.has_conflicts() {
      return Err(anyhow!(
        "The staged changes cannot be committed because they would result in a conflict. \
        Check with \"git diff --cached {}\", and adjust staged changes accordingly.",
        target.display_name
      ));
    }

    let index_tree_id = index.write_tree_to(repo)?;
    let tree = repo.find_tree(index_tree_id)?;

    let mut diff = repo.diff_tree_to_tree(Some(&target_tree), Some(&tree), None)?;
    diff.find_similar(None)?;

    Ok((tree, diff))
  }

  // HOOKS / MSG PROCESSING

  /// Gets the initial commit message file and type. Does not account for
  /// amends. Amend message source should be resolved earlier.
  fn get_msg_source(&self, repo: &Repository, ty: CommitType) -> Result<MsgSource> {
    if !self.words.is_empty() {
      return Ok(MsgSource {
        file_name: "COMMIT_EDITMSG".to_string(),
        ty: MsgSourceType::Message,
      });
    }

    if let CommitType::Amend(id) = ty {
      return Ok(MsgSource {
        file_name: "COMMIT_EDITMSG".to_string(),
        ty: MsgSourceType::Commit(id),
      });
    };

    if ty == CommitType::Merge {
      return Ok(MsgSource {
        file_name: "MERGE_MSG".to_string(),
        ty: MsgSourceType::Merge,
      });
    }

    let git_dir = repo.path();

    if git_dir.join("SQUASH_MSG").exists() {
      return Ok(MsgSource {
        file_name: "SQUASH_MSG".to_string(),
        ty: MsgSourceType::Squash,
      });
    }

    if git_dir.join("rebase-merge").exists() {
      let mut rebase = repo.open_rebase(None)?;
      let i = rebase
        .operation_current()
        .context("Failed to get current rebase operation index")?;

      let op = rebase
        .nth(i)
        .context("Failed to get current rebase operation")?;

      return Ok(MsgSource {
        file_name: "rebase-merge/message".to_string(),
        ty: MsgSourceType::Commit(op.id()),
      });
    }

    if git_dir.join("rebase-apply").exists() {
      let mut rebase = repo.open_rebase(None)?;
      let i = rebase
        .operation_current()
        .context("Failed to get current rebase operation index")?;

      let op = rebase
        .nth(i)
        .context("Failed to get current rebase operation")?;

      return Ok(MsgSource {
        file_name: "rebase-apply/message".to_string(),
        ty: MsgSourceType::Commit(op.id()),
      });
    }

    Ok(MsgSource {
      file_name: "COMMIT_EDITMSG".to_string(),
      ty: MsgSourceType::None,
    })
  }

  /// Runs prepare-commit-msg hook
  fn prepare_msg_hook(&self, repo: &Repository, source: &MsgSource) -> Result<()> {
    let script = repo.path().join("hooks").join("prepare-commit-msg");
    if !script.exists() {
      return Ok(());
    }

    let mut args: Vec<OsString> = Vec::new();
    args.push(repo.path().join("COMMIT_EDITMSG").into());

    if source.ty != MsgSourceType::None {
      args.push(source.ty.to_string().into());

      if let MsgSourceType::Commit(id) = source.ty {
        args.push(id.to_string().into());
      }
    }

    let output = Command::new(script).args(&args).output()?;

    if output.status.success() {
      Ok(())
    } else {
      eprintln!("prepare-commit-msg hook {}", style("failed!").red());
      eprintln!("{}", output.stderr.to_str_lossy());
      Err(anyhow!("prepare-commit-msg hook failed"))
    }
  }

  /// Opens the file COMMIT_EDITMSG in the editor and returns the resulting
  /// message with all comment lines removed. COMMIT_EDITMSG should contain the
  /// correct contents before this point.
  fn invoke_editor(&self, repo: &Repository) -> Result<()> {
    let editor = get_editor(repo)?;
    let editmsg = repo.path().join("COMMIT_EDITMSG");

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
      return Err(if let Some(code) = status.code() {
        anyhow!("Editor failed with code: {}", code)
      } else {
        anyhow!("Editor failed")
      });
    }

    Ok(())
  }

  /// Runs commit-msg hook
  fn commit_msg_hook(&self, repo: &Repository) -> Result<()> {
    if self.no_verify {
      println!("{} commit-msg hook", style("Skipping").yellow());
      return Ok(());
    }

    let script = repo.path().join("hooks/commit-msg");
    if !script.exists() {
      return Ok(());
    }

    let output = Command::new(script)
      .arg(repo.path().join("COMMIT_EDITMSG"))
      .output()?;

    if output.status.success() {
      Ok(())
    } else {
      eprintln!("commit-msg hook {}", style("failed!").red());
      eprintln!("{}", output.stderr.to_str_lossy());
      Err(anyhow!("commit-msg hook failed"))
    }
  }

  /// Reads the commit msg from COMMIT_EDITMSG and removes comment lines
  fn read_commit_msg(&self, editmsg: &Path) -> Result<String> {
    let file = fs::read_to_string(editmsg)?;
    let mut out = Vec::new();

    for line in file.lines() {
      if !line.starts_with('#') {
        out.push(line);
      }
    }

    Ok(out.join("\n"))
  }

  /// Runs pre-commit hook
  fn pre_commit_hook(&self, repo: &Repository) -> Result<()> {
    if self.no_verify {
      println!("{} pre-commit hook", style("Skipping").yellow());
      return Ok(());
    }

    let script = repo.path().join("hooks").join("pre-commit");
    if !script.exists() {
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
      eprintln!("\n{}", output.stderr.to_str_lossy());
      Err(anyhow!("Precommit hook failed"))
    }
  }

  /// Runs post-commit hook
  fn post_commit_hook(&self, repo: &Repository) -> Result<()> {
    let script = repo.path().join("hooks").join("post-commit");

    if script.exists() {
      // post-commit doesn't affect outcome of commit, nor does it do any meaningful
      // processing
      let mut cmd = Command::new(script).spawn()?;

      let status = cmd.wait()?;
      if !status.success() {
        eprintln!("{}", style("post-commit hook failed").dim());
      }
    }

    Ok(())
  }

  /// Run post-rewrite hook
  fn post_rewrite_hook(&self, repo: &Repository, old_id: Oid, new_id: Oid) -> Result<()> {
    let input = format!("{} {}", old_id, new_id);
    let script = repo.path().join("hooks/post-rewrite");

    if script.exists() {
      let mut cmd = Command::new(script)
        .arg("amend")
        .stdin(Stdio::piped())
        .spawn()?;

      cmd
        .stdin
        .take()
        .context("Failed to open stdin of post-rewrite hook")?
        .write_all(input.as_bytes())?;

      let status = cmd.wait()?;
      if !status.success() {
        eprintln!("{}", style("post-rewrite hook failed").dim());
      }
    }

    Ok(())
  }
}

/// Finds the configured editor, matching git's search order
fn get_editor(repo: &Repository) -> Result<String> {
  // 1. GIT_EDITOR env var
  match env::var("GIT_EDITOR") {
    Ok(it) => return Ok(it),
    Err(VarError::NotPresent) => {}
    Err(e) => return Err(e.into()),
  };

  // 2. core.editor config var
  let config = repo.config()?.snapshot()?;
  match config.get_string("core.editor") {
    Ok(it) => return Ok(it),
    Err(e) if e.code() == ErrorCode::NotFound => {}
    Err(e) => return Err(e.into()),
  };

  // 3, 4. VISUAL, EDITOR env vars
  for var in ["VISUAL", "EDITOR"] {
    match env::var(var) {
      Ok(it) => return Ok(it),
      Err(VarError::NotPresent) => {}
      Err(e) => return Err(e.into()),
    };
  }

  // 5. platform default
  Ok(if cfg!(windows) {
    "notepad".into()
  } else {
    "vi".into()
  })
}

/// The source of the commit message going into the editor and
/// prepare-commit-msg hook.
#[derive(PartialEq)]
enum MsgSourceType {
  /// A non-empty message was specified in the command line. All other types
  /// imply an empty message in the command line.
  Message,

  // /// Use a template file
  // Template,
  /// MERGE_MSG
  Merge,

  /// SQUASH_MSG
  Squash,

  /// An existing commit (e.g. amend)
  Commit(Oid),

  /// No default msg, editor must be invoked
  None,
}

impl Display for MsgSourceType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      MsgSourceType::Message => write!(f, "message"),
      // MsgSourceType::Template => write!(f, "template"),
      MsgSourceType::Merge => write!(f, "merge"),
      MsgSourceType::Squash => write!(f, "squash"),
      MsgSourceType::Commit(_) => write!(f, "commit"),
      MsgSourceType::None => Ok(()),
    }
  }
}

/// The source of the default commit message
struct MsgSource {
  /// The file that contains the default message
  file_name: String,
  /// The type of the default message
  ty: MsgSourceType,
}

/// Builds the content of COMMIT_EDITMSG before it's opened in an editor
///
/// # Params
/// - `initial` - the pre-filled commit message (should usually end with a
///   newline)
/// - `to` - a user-friendly name for where the commit is being applied. None
///   when there are no commits in the repo
/// - `diff` - the changes in the commit
///
/// # Returns
/// The text that should populate COMMIT_EDITMSG
fn build_msg_template(initial: &[u8], to: Option<&str>, diff: &Diff) -> Result<Vec<u8>> {
  let mut out: Vec<u8> = Vec::with_capacity(1000);
  out.extend_from_slice(initial);

  if let Some(to) = to {
    out.extend_from_slice(format!("\n# Committing on {}", to).as_bytes());
  } else {
    out.extend_from_slice(b"\n# Initial commit");
  }

  let summary = DiffSummary::new(diff)?;
  let summary = display_summary(&summary);

  for line in summary.lines() {
    out.extend_from_slice(format!("\n# {}", strip_ansi_codes(line)).as_bytes());
  }

  Ok(out)
}

/// Displays the header-line for a regular commit
///
/// Committed hash to branch as Author Name
fn display_commit_header(target: Option<&str>) -> Result<String> {
  Ok(format!(
    "{} to {}",
    style("Committed").green(),
    match target {
      Some(it) => style(it).blue(),
      None => style("unknown").red(),
    }
  ))
}

/// Displays the header-line for an amend
///
/// `Amended <old hash> on <branch> as <Author Name>`
fn display_amend_header(old_commit: &Commit, target: &str) -> Result<String> {
  Ok(format!(
    "{} {} on {}",
    style("Amended").green(),
    display_hash(old_commit.as_object())?,
    style(target).blue()
  ))
}

/// Displays the header line for a merge commit
///
/// `Merged <base> into <branch>: <hash> as <Author Name>`
fn display_merge_header(
  repo: &Repository,
  merge_head: &Reference,
  head: Option<&str>,
) -> Result<String> {
  let merge_commit = merge_head.peel_to_commit()?;
  let from = style(resolve_commit_name(repo, &merge_commit)?).blue();

  let to = match head {
    Some(it) => style(it).magenta().to_string(),
    None => get_current_branch_name(repo)?
      .map(|it| style(it).magenta().to_string())
      .unwrap_or(style("unknown").red().to_string()),
  };

  Ok(format!("{} {} into {}", style("Merged").green(), from, to))
}

/// Displays the remaining commit details in the same format as `feature show`,
/// with two exceptions:
/// 1. The time is always absolute
/// 2. It always displays the entire commit message
fn display_commit_details(commit: &Commit<'_>, diff: &Diff, config: &UserConfig) -> Result<String> {
  let commit_output = display_commit(commit, &DisplayCommitOptions {
    time: DisplayTimeOptions {
      // relative is not useful, commit just occured
      relative: false,
      fmt: config.format_date()?,
    },
    // want the user to see the entire message just for reference
    message: CommitMessageLevel::Full,
  })?;

  let summary = DiffSummary::new(diff)?;
  Ok(format!(
    "{}\n\n{}",
    commit_output,
    display_summary(&summary)
  ))
}
