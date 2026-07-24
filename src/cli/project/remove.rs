use anyhow::{Context, Result};
use console::style;

use crate::App;
use crate::core::project::Project;

const LONG_ABOUT: &str = r#"Remove a project from this repo.

This simply removes the metadata to track the project (entries in feature.toml
and .gitignore). To protect against data loss, it doesn't delete the repo
itself."#;

#[derive(clap::Args, Debug)]
#[command(
  about = "Remove a project from this repo",
  visible_alias = "rm",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct RemoveArgs {
  /// The name of the project
  name: String,
}

impl RemoveArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let project = Project::open(&self.name, &state.config)?
      .with_context(|| format!("Failed to find project named \"{}\"", &self.name))?;

    let parent_root = state.repo.workdir().unwrap_or_else(|| state.repo.path());
    project.unlink(parent_root, &state.config_path)?;

    println!("{} {}", style("Removed").red(), &self.name);
    Ok(())
  }
}
