use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{Context, Result};
use console::style;
use toml_edit::DocumentMut;

use crate::App;

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
    let project_path = self.remove_config_entry(&state.config_path)?;

    self.remove_gitignore_entry(
      state.repo.workdir().unwrap_or_else(|| state.repo.path()),
      &project_path,
    )?;

    Ok(())
  }

  /// Removes the entry from the project config file and return's the path of
  /// the subproject.
  fn remove_config_entry(&self, config_path: &Path) -> Result<String> {
    let mut file = OpenOptions::new()
      .read(true)
      .write(true)
      .create(false)
      .truncate(false)
      .open(config_path)?;

    file.lock()?;

    let mut toml = String::new();
    file.read_to_string(&mut toml)?;
    let mut doc = toml.parse::<DocumentMut>()?;

    let mut old = doc["projects"]
      .as_table_like_mut()
      .context("Failed to get 'projects' as table!")?
      .remove(&self.name)
      .with_context(|| format!("No config entry for '{}'!", &self.name))?;
    let old = old
      .as_table_like_mut()
      .with_context(|| format!("'{}' is not a table!", &self.name))?;

    let toml = doc.to_string();
    let toml = toml.as_bytes();

    file.seek(SeekFrom::Start(0))?;
    file.set_len(toml.len() as u64)?;
    file.write_all(toml)?;
    file.unlock()?;

    // need path to remove gitignore entry
    let path = old
      .get("path")
      .with_context(|| format!("No value for 'path' in '{}'", &self.name))?;
    let path_string = path.as_str().context("'path' is not a string!")?.to_owned();
    println!(
      "{} {} from {}",
      style("Removed").red(),
      style(&self.name).cyan(),
      config_path.to_string_lossy()
    );
    if let Some(url) = old.get("url") {
      println!("  url ={}", url);
    }
    println!("  path ={}", path);

    Ok(path_string)
  }

  fn remove_gitignore_entry(&self, root_path: &Path, project_path: &str) -> Result<()> {
    let path = root_path.join(".gitignore");
    if !path.exists() {
      return Ok(());
    }

    let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
    file.lock()?;

    let mut ignore = String::new();
    file.read_to_string(&mut ignore)?;

    let mut new_ignore = ignore
      .lines()
      .filter(|line| *line != project_path)
      .collect::<Vec<_>>()
      .join("\n");
    new_ignore.push('\n');

    file.seek(SeekFrom::Start(0))?;
    file.set_len(new_ignore.len() as u64)?;
    file.write_all(new_ignore.as_bytes())?;
    file.unlock()?;

    println!(
      "{} \"{}\" from .gitignore",
      style("Removed").red(),
      style(project_path).cyan()
    );
    Ok(())
  }
}
