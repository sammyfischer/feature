use anyhow::Result;
use console::style;

use crate::App;

#[derive(clap::Args, Debug)]
#[command(
  about = "List all projects in this repo",
  visible_alias = "ls",
  disable_help_subcommand = true
)]
pub struct ListArgs {}

impl ListArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    for (name, project) in &state.config.projects {
      println!("{}", style(name).cyan());
      println!("  url = {}", project.url);
      println!("  path = {}", project.path.to_string_lossy());
    }
    Ok(())
  }
}
