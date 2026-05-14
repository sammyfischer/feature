use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use console::style;
use git2::{ErrorClass, ErrorCode, Repository};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::App;

const LONG_ABOUT: &str = r"Interact with feature projects.

Feature projects are a way of including other git repos as subprojects of this
repo. This allows you to create a monorepo out of a project that spans multiple
repos.";

const ADD_LONG_ABOUT: &str = r"Add a project to this repo.

If you've already cloned the project, just specify the path and name. If not,
you can specify the url and the project will automatically be cloned.

If you omit the path, feature will attempt to create a dir with the name of the
project in this repo's root.";

const RM_LONG_ABOUT: &str = r#"Remove a project from this repo.

This simply removes all traces of the project from this repo (metadata in
feature.toml and .gitignore). To also (irreversibly) delete the repo, add the
"-d" option."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Interact with projects",
  long_about = LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  #[command(subcommand)]
  command: ProjectCommand,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ProjectCommand {
  Add(AddArgs),
  Rm(RmArgs),
}

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Add a project to this repo",
  long_about = ADD_LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct AddArgs {
  /// The repo url of the project
  #[arg(long)]
  url: Option<String>,

  /// The path to the subproject root. This must be relative to the superproject root.
  #[arg(long)]
  path: Option<String>,

  /// The name of the project
  name: String,
}

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Remove a project from this repo",
  long_about = RM_LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct RmArgs {
  /// Also delete the repo dir (irreversible)
  #[arg(short, long)]
  delete: bool,

  /// The name of the project
  name: String,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    match &self.command {
      ProjectCommand::Add(args) => args.run(state),
      ProjectCommand::Rm(args) => args.run(state),
    }
  }
}

struct ProjectInfo {
  name: String,
  url: String,
  path: PathBuf,
}

impl AddArgs {
  fn run(&self, state: &App) -> Result<()> {
    let parent_root = state.repo.workdir().unwrap_or_else(|| state.repo.path());

    let project = match (self.url.as_ref(), self.path.as_ref()) {
      // neither, check if dir called `name` exists and use that
      (None, None) => {
        let path = PathBuf::from(&self.name);

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

    Ok(())
  }

  fn add_from_existing(&self, repo: Repository, path: PathBuf) -> Result<ProjectInfo> {
    let remotes = repo.remotes()?;
    let remote_name = remotes
      .iter()
      .flatten()
      .next()
      .context("Couldn't find a remote url to use!")?;

    let remote = repo.find_remote(remote_name)?;
    let url = remote.url().context("Remote url is not valid utf-8!")?;

    Ok(ProjectInfo {
      name: self.name.clone(),
      url: url.to_owned(),
      path,
    })
  }

  fn clone_and_add(&self, url: String, path: PathBuf) -> Result<ProjectInfo> {
    Repository::clone(&url, &path)?;

    Ok(ProjectInfo {
      name: self.name.clone(),
      url,
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

    doc["projects"][&project.name]["url"] = value(&project.url);
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
    Ok(())
  }

  fn write_to_gitignore(&self, root: &Path, project: &ProjectInfo) -> Result<()> {
    let entry = project
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

    if !ignore.ends_with('\n') && !ignore.is_empty() {
      ignore.push('\n');
    }
    ignore.push_str(&format!("{}\n", entry));

    file.seek(SeekFrom::Start(0))?;
    file.set_len(ignore.len() as u64)?;
    file.write_all(ignore.as_bytes())?;
    file.unlock()?;
    println!("Added {} to .gitignore", style(entry).cyan());
    Ok(())
  }
}

impl RmArgs {
  fn run(&self, _state: &App) -> Result<()> {
    Ok(())
  }
}
