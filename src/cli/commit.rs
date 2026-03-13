//! Commit subcommand

use crate::cli::CliResult;
use crate::cli::error::CliError;
use crate::{await_child, git};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// Whether to amend the previous commit
  #[arg(
    long,
    long_help = "Amend the previous commit. Remaining args overwrite the previous commit message. If no remaining args are specified, the previous commit message is preserved."
  )]
  amend: bool,

  /// Words to join together as commit message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  words: Vec<String>,
}

impl Args {
  pub fn run(&self) -> CliResult {
    let commit_msg = self.words.join(" ");

    if self.amend {
      let mut cmd = git!("commit", "--amend");

      if commit_msg.is_empty() {
        cmd.arg("--no-edit");
      } else {
        cmd.args(["-m", &commit_msg]);
      };

      let mut child = cmd.spawn()?;
      return await_child!(child, "Failed to amend commit");
    }

    if commit_msg.is_empty() {
      return Err(CliError::Generic("Must specify a commit message".into()));
    }

    await_child!(
      git!("commit", "-m", commit_msg).spawn()?,
      "Failed to create commit"
    )
  }
}
