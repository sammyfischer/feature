use std::path::Path;

use anyhow::Result;
use console::style;

use crate::core::project::Project;
use crate::{App, style};

const LONG_ABOUT: &str = r"Add a project to this repo.

If you've already cloned the project, just specify the path and name. If not,
you can specify the url and the project will automatically be cloned.

If you omit the path, feature will attempt to create a dir with the name of the
project in this repo's root.";

#[derive(clap::Args, Debug)]
#[command(
  about = "Add a project to this repo",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct AddArgs {
  /// The repo url
  #[arg(short = 'r', long)]
  url: Option<String>,

  /// The path where the subproject will reside, relative to this repo.
  #[arg(short, long)]
  path: Option<String>,

  /// The name of the project
  name: String,
}

impl AddArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let parent_root = state.repo.workdir().unwrap_or_else(|| state.repo.path());
    let parent_config = &state.config_path;

    let path = self.path.as_ref().map(Path::new);
    let url = self.url.as_deref();

    let project = Project::create(&self.name, path, url, parent_root, parent_config)?;

    println!(
      "{} project {}",
      style("Created").green(),
      style(project.name()).cyan()
    );

    println!(
      "  in {}",
      style!(
        "./{}",
        project.path().to_str().expect("Path should be utf-8")
      )
      .blue()
    );

    println!("  from {}", style(&project.url()).magenta());

    Ok(())
  }
}
