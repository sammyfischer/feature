use std::io::IsTerminal;
use std::str::Lines;

use anyhow::{Context, Result};
use console::{style, truncate_str};

use crate::cli::{Cli, get_term_width, paginate};
use crate::git;

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
#[command(about = "View a graph of commits", long_about = LONG_ABOUT)]
pub struct Args {
  /// The format passed to git log
  #[arg(long, long_help = FORMAT_LONG_HELP)]
  format: Option<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    // like log, but author name and date are first and colored
    let output = git!(
      "log",
      "--graph",
      "--all",
      "--color=always",
      format!(
        "--pretty={}",
        self.format.as_ref().unwrap_or(&cli.config.format.graph)
      ),
    )
    .output()
    .context("Failed to get git output")?;

    let string_output = String::from_utf8(output.stdout).expect("Git output should be valid utf-8");

    // if stdout is not a terminal, just print and return
    if !std::io::stdout().is_terminal() {
      println!("{}", string_output);
      return Ok(());
    }

    // if stdout is a terminal, truncate lines
    let truncated = truncate_lines(&mut string_output.lines()).join("\n");

    paginate(&truncated)
  }
}

fn truncate_lines(lines: &mut Lines) -> Vec<String> {
  let mut out: Vec<String> = Vec::new();
  let term_width = get_term_width();
  let tail = style("\u{2026}").dim().to_string();

  // truncate each line to term width
  for line in lines {
    out.push(truncate_str(line, term_width, &tail).to_string());
  }

  out
}
