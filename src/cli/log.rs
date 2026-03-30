use anyhow::Result;

use crate::cli::Cli;
use crate::{await_child, git};

const LONG_ABOUT: &str = r"View commit logs.

The aim of this command is to view and inspect all
commits. Unlike graph, this does not truncate lines.

The default format shows a short hash, branch/HEAD info, commit subject line,
and author name and time.";

const FORMAT_LONG_HELP: &str = r#"This format is passed in as the value of "--pretty".
See the PRETTY FORMATS section of git log --help for more information on how to
customize this."#;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "View commit logs", long_about = LONG_ABOUT)]
pub struct Args {
  /// The format passed to git log
  #[arg(long, long_help = FORMAT_LONG_HELP)]
  format: Option<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    // default pretty format:
    // %h = hash, %d = decorator (e.g. branch pointing to that commit)
    // %s = subject (commit description title line)
    // %an = author name, %ar = author date (relative)
    await_child!(
      git!(
        "log",
        "--all",
        format!(
          "--pretty={}",
          self.format.as_ref().unwrap_or(&cli.config.format.log)
        )
      )
      .spawn()
      .expect("Failed to call git"),
      "Failed to call git"
    )
  }
}
