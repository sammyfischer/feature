//! Start subcommand

use crate::cli::{Cli, CliResult};
use crate::{await_child, git};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  #[arg(long, default_value = "-")]
  /// The separator to use when joining words
  pub sep: Option<String>,

  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
  /// Words to join together as branch name
  pub words: Vec<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let sep = self.sep.as_ref().unwrap_or(&cli.config.branch_sep);

    let branch_name = self.words.join(sep);
    println!("Creating branch: {}", branch_name);

    await_child!(
      git!("switch", "-c", branch_name).spawn()?,
      "git failed to execute"
    )
  }
}
