use std::process::Command;

use anyhow::{Context, Result};
use clap::ValueHint;
use console::style;

use crate::App;

#[derive(clap::Args, Debug)]
#[command(
  about = "Run a command in each project",
  disable_help_subcommand = true
)]
pub struct EachArgs {
  /// Filter projects by prefix of name (comma-separate for multiple)
  #[arg(short, long, value_delimiter = ',')]
  pub filter: Vec<String>,

  #[arg(
    trailing_var_arg = true,
    allow_hyphen_values = true,
    required = true,
    value_hint = ValueHint::CommandWithArguments
  )]
  pub args: Vec<String>,
}

impl EachArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let root = state.repo.workdir().unwrap_or_else(|| state.repo.path());
    let mut first = true;

    // run sequentially rather than in parallel bc we have no knowledge of the
    // command being run
    for (name, project) in &state.config.projects {
      // if any filters were specified
      if !self.filter.is_empty() {
        // if name doesn't start with one of the filters
        if !self.filter.iter().any(|filter| name.starts_with(filter)) {
          continue;
        }
      }

      if !first {
        println!();
      }
      first = false;
      println!("{}", style(name).bold().cyan());

      let mut cmd_line = self.args.iter();
      let cmd = cmd_line.next().context("Must specify a command!")?;
      let args: Vec<&String> = cmd_line.collect();

      let st = Command::new(cmd)
        .current_dir(root.join(&project.path))
        .args(args)
        .status()
        .with_context(|| {
          format!(
            "Failed to run project '{}' (path: {})",
            name,
            project.path.to_string_lossy()
          )
        })?;

      if st.success() {
        println!("{} {}", style(name).cyan(), style("succeeded").green());
      } else {
        println!("{} {} {}", style(name).cyan(), style("failed").red(), st);
      }
    }
    Ok(())
  }
}
