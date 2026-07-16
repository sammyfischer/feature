use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{ErrorClass, ErrorCode, Repository};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::core::NotFoundExt;
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
  /// The repo uri
  #[arg(long, value_name = "URI")]
  repo: Option<String>,

  /// The path where the subproject will reside, relative to this repo.
  #[arg(long)]
  path: Option<String>,

  /// The name of the project
  name: String,
}

struct ProjectInfo {
  name: String,
  uri: String,
  path: PathBuf,
}

impl AddArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let parent_root = state.repo.workdir().unwrap_or_else(|| state.repo.path());

    let project = match (self.repo.as_ref(), self.path.as_ref()) {
      // neither, check if dir called `name` exists and use that
      (None, None) => {
        let path = PathBuf::from(&self.name);

        let repo = Repository::open(&path)
          .repo_not_found_ok()?
          .with_context(|| format!("Repo not found at: {}", path.to_string_lossy()))?;

        self.add_from_existing(repo, path)
      }

      // path only, assume it's a repo already
      (None, Some(path)) => {
        let path = PathBuf::from(path);

        let repo = match Repository::open(&path) {
          Ok(it) => it,
          Err(e) if e.class() == ErrorClass::Repository && e.code() == ErrorCode::NotFound => {
            return Err(anyhow!(
              "No repo found at path: {}",
              &path.to_string_lossy()
            ));
          }
          Err(e) => return Err(e.into()),
        };

        self.add_from_existing(repo, path)
      }

      // url only, create dir called `name` and clone repo
      (Some(url), None) => {
        let path = PathBuf::from(&self.name);

        match fs::create_dir(&path) {
          Ok(_) => (),
          Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
          Err(e) => return Err(e.into()),
        };

        self.clone_and_add(url.to_owned(), path)
      }

      // both, create dir and clone repo
      (Some(url), Some(path)) => {
        let path = PathBuf::from(path);

        match fs::create_dir_all(&path) {
          Ok(_) => (),
          Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
          Err(e) => return Err(e.into()),
        };

        self.clone_and_add(url.to_owned(), path)
      }
    }?;

    self.write_to_config(&state.config_path, &project)?;
    self.write_to_gitignore(parent_root, &project)?;

    println!(
      "{} project {}",
      style("Created").green(),
      style(&project.name).cyan()
    );
    println!(
      "  in {}",
      style!(
        "./{}",
        &project.path.to_str().expect("Path should be utf-8")
      )
      .blue()
    );
    println!("  from {}", style(&project.uri).magenta());

    Ok(())
  }

  fn add_from_existing(&self, repo: Repository, path: PathBuf) -> Result<ProjectInfo> {
    let remotes = repo.remotes()?;
    let remote_name = remotes
      .iter()
      .flatten()
      .next()
      .context("Couldn't find a remote url to use!")?
      .context("Remote names must be valid utf-8")?;

    let remote = repo.find_remote(remote_name)?;
    let url = remote.url().context("Remote url is not valid utf-8!")?;

    Ok(ProjectInfo {
      name: self.name.clone(),
      uri: url.to_owned(),
      path,
    })
  }

  fn clone_and_add(&self, uri: String, path: PathBuf) -> Result<ProjectInfo> {
    Repository::clone(&uri, &path)
      .context("Attempting to clone because \"--repo\" was specified")?;

    Ok(ProjectInfo {
      name: self.name.clone(),
      uri,
      path,
    })
  }

  fn write_to_config(&self, path: &Path, project: &ProjectInfo) -> Result<()> {
    // ensure parent dirs exist
    if let Some(parent) = path.parent() {
      match fs::create_dir_all(parent) {
        Ok(_) => (),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
      };
    }

    // open or create file in rw mode
    let mut file = OpenOptions::new()
      .read(true)
      .write(true)
      .create(true)
      .truncate(false)
      .open(path)?;

    file.lock()?;

    let mut toml = String::new();
    file.read_to_string(&mut toml)?;

    let mut doc = if toml.is_empty() {
      DocumentMut::new()
    } else {
      toml.parse::<DocumentMut>()?
    };

    // ensure [projects] exists
    if doc.get("projects").is_none() {
      doc.insert("projects", Item::Table(Table::new()));
    }

    doc["projects"][&project.name]["url"] = value(&project.uri);
    doc["projects"][&project.name]["path"] = value(
      project
        .path
        .to_str()
        .context("Project path must be valid utf-8!")?,
    );

    let toml = doc.to_string();
    let toml = toml.as_bytes();

    file.seek(SeekFrom::Start(0))?;
    file.set_len(toml.len() as u64)?;
    file.write_all(toml)?;
    file.unlock()?;

    println!(
      "{} entry to {}",
      style("Added").green(),
      style(path.to_string_lossy()).cyan()
    );

    Ok(())
  }

  fn write_to_gitignore(&self, root: &Path, project: &ProjectInfo) -> Result<()> {
    let path_string = project
      .path
      .to_str()
      .context("Project path must be valid utf-8!")?;

    let mut file = OpenOptions::new()
      .read(true)
      .write(true)
      .create(true)
      .truncate(false)
      .open(root.join(".gitignore"))?;
    file.lock()?;

    let mut ignore = String::new();
    file.read_to_string(&mut ignore)?;

    if ignore.lines().any(|line| line == path_string) {
      file.unlock()?;
      return Ok(());
    }

    if !ignore.ends_with('\n') && !ignore.trim().is_empty() {
      ignore.push('\n');
    }
    ignore.push_str(&format!("{}\n", path_string));

    file.seek(SeekFrom::Start(0))?;
    file.set_len(ignore.len() as u64)?;
    file.write_all(ignore.as_bytes())?;
    file.unlock()?;

    println!(
      "{} \"{}\" to .gitignore",
      style("Added").green(),
      style(path_string).cyan()
    );
    Ok(())
  }
}
