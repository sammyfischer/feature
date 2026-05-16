use anyhow::{Context, Result};
use clap::ValueHint;

use crate::util::term::paginate;
use crate::{App, data, git};

const LONG_ABOUT: &str = r"View a graph of commits.

The aim of this command is to visualize commit history, rather than view and
find specific commits. For this reason, output is more colorful and truncated to
a single line per commit.

Uses git log --graph under the hood.

The default format shows a short hash, branch/HEAD info, author name and time,
and as much of the commit subject line as will fit.";

const FORMAT_LONG_HELP: &str = r#"This format is passed in as the value of "--pretty".
See the PRETTY FORMATS section of git log --help for more information on how to
customize this."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "View a graph of commits",
  long_about = LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// The format passed to git log
  #[arg(long, visible_alias = "fmt", long_help = FORMAT_LONG_HELP, value_hint = ValueHint::Other)]
  format: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let fmt = match self.format.as_ref() {
      // use cli opt
      Some(fmt) => Some(fmt.to_owned()),

      // try to get config field
      None => {
        let config = state.repo.config()?.snapshot()?;
        data::get_format_graph(&config)?
      }
    };

    let mut cmd = match fmt {
      Some(fmt) => git!(
        "log",
        "--graph",
        "--all",
        "--color=always",
        format!("--pretty={}", fmt)
      ),
      None => git!("log", "--graph", "--all", "--color=always"),
    };

    let output = cmd.output().context("Failed to get git output")?;
    paginate(&output.stdout)?;
    Ok(())
  }
}
