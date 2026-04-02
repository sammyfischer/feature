use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};
use std::str::Lines;

use ansi_parser::AnsiParser;
use ansi_parser::Output::{Escape, TextBlock};
use anyhow::{Context, Result};
use unicode_width::UnicodeWidthChar;

use crate::cli::{Cli, get_term_width};
use crate::{await_child, git};

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

    // forward output to less
    // -F = just print output to stdout if it fits the terminal height
    // -R = render control chars as-is
    let mut less_proc = Command::new("less")
      .arg("-FR")
      .stdin(Stdio::piped())
      .spawn()
      .expect("Failed to start less");

    let stdin = less_proc
      .stdin
      .as_mut()
      .expect("Failed to send output to less");

    stdin
      .write_all(truncated.as_bytes())
      .expect("Failed to send output to less");

    await_child!(less_proc, "Less")
  }
}

fn truncate_lines(lines: &mut Lines) -> Vec<String> {
  let mut out: Vec<String> = Vec::new();
  let term_width = get_term_width();

  // truncate each line to term width
  for line in lines {
    // output buffer
    let mut line_buf = String::new();

    // accumulated line width
    let mut acc_width = 0usize;

    // whether the current line was truncated
    let mut truncated = false;

    'tokens: for token in line.ansi_parse() {
      match token {
        TextBlock(text) => {
          // push chars until terminal width
          for c in text.chars() {
            let char_width = c.width().unwrap_or(0);

            if acc_width + char_width > term_width {
              truncated = true;
              break 'tokens;
            }

            acc_width += char_width;
            line_buf.push(c);
          }
        }

        Escape(ansi_sequence) => {
          // always add
          line_buf.push_str(&ansi_sequence.to_string());
        }
      }
    }

    if truncated {
      // replace end with ellipsis
      line_buf.pop();
      line_buf.push('\u{2026}');
      // reset color/formatting
      line_buf.push_str("\x1b[0m");
    }

    // push line to output
    out.push(line_buf);
  }

  out
}
