use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};

use ansi_parser::AnsiParser;
use ansi_parser::Output::{Escape, TextBlock};
use anyhow::Result;
use unicode_width::UnicodeWidthChar;

use crate::cli::get_term_width;
use crate::{await_child, git};

pub fn graph() -> Result<()> {
  // like log, but author name and date are first and colored
  let output = git!(
    "log",
    "--graph",
    "--all",
    "--color=always",
    "--pretty=format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%s",
  )
  .output()
  .expect("Failed to get git output");

  let string_output = String::from_utf8(output.stdout).expect("Git output should be valid utf-8");

  // if stdout is not a terminal, just print and return
  if !std::io::stdout().is_terminal() {
    println!("{}", string_output);
    return Ok(());
  }

  // if stdout is a terminal, truncate lines

  let lines = string_output.lines();
  let mut out_lines: Vec<String> = Vec::new();

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
    out_lines.push(line_buf);
  }

  let truncated = out_lines.join("\n");

  // forward output to less
  // -F = just print output to stdout if it fits the terminal height
  // -R = render control chars as-is
  let mut less_proc = Command::new("less")
    .arg("-FR")
    .stdin(Stdio::piped())
    .spawn()
    .expect("Failed to start pager");

  let stdin = less_proc
    .stdin
    .as_mut()
    .expect("Failed to send output to pager");

  stdin
    .write_all(truncated.as_bytes())
    .expect("Failed to send output to pager");
  await_child!(less_proc, "Pager failed")
}
