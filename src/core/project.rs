use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::build::RepoBuilder;
use git2::{FetchOptions, RemoteCallbacks, Repository};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::core::fetch::get_credentials_cb;
use crate::core::project_config::ProjectConfig;
use crate::core::user_config::UserConfig;
use crate::core::{NotFoundExt, project_config};

pub struct Project {
  name: String,
  path: PathBuf,
  url: String,
  repo: Repository,
}

impl Project {
  /// The name of the project metadata file stored in the git dir. This file
  /// should exist in a project, and must not exist in a non-project git dir.
  pub const METADATA_FILE: &str = "feature-project";

  /// Opens an existing project from the given config. Returns `None` if the
  /// repo doesn't exist.
  pub fn open(name: &str, parent_config: &ProjectConfig) -> Result<Option<Self>> {
    let entry = &parent_config.projects[name];
    let path = &entry.path;
    let url = &entry.url;

    let Some(repo) = Repository::open(path).repo_not_found_ok()? else {
      return Ok(None);
    };

    Ok(Some(Self {
      name: name.to_owned(),
      path: path.to_owned(),
      url: url.to_owned(),
      repo,
    }))
  }

  /// Opens an existing project from the given config. Errors if the repo
  /// doesn't exist.
  pub fn open_existing(name: &str, parent_config: &ProjectConfig) -> Result<Self> {
    let entry = &parent_config.projects[name];
    let path = &entry.path;
    let url = &entry.url;
    let repo = Repository::open(path)?;

    Ok(Self {
      name: name.to_owned(),
      path: path.to_owned(),
      url: url.to_owned(),
      repo,
    })
  }

  /// Opens an existing project, making sure to clone it and write all metadata.
  pub fn open_and_init(
    name: &str,
    parent_config: &ProjectConfig,
    parent_root: &Path,
  ) -> Result<Self> {
    let entry = &parent_config.projects[name];
    let this = Self::clone_project(name, &entry.path, &entry.url)?;
    this.ensure_metadata(parent_root)?;
    Ok(this)
  }

  /// Creates a new project. This will create all necessary dirs, clone the repo
  /// if it doesn't exist, and write all metadata.
  ///
  /// # Params
  /// - `name` - the name of the project
  /// - `path` - the path to the project repo. Defaults to a directory with the
  ///   name of the project, in the parent project's root.
  /// - `url` - the url of the project repo. When not specified, it's assumed
  ///   that a repo exists in `path`.
  /// - `parent_root` - the parent project's root directory
  /// - `parent_config` - the parent project's config file, relative to its root
  pub fn create(
    name: &str,
    path: Option<&Path>,
    url: Option<&str>,
    parent_root: &Path,
    parent_config: &Path,
  ) -> Result<Self> {
    let path = path.unwrap_or(Path::new(name));

    let this = match url {
      // clone
      Some(url) => Self::clone_project(name, path, url)?,

      // assume repo exists
      None => {
        let repo = Repository::open(path)?;
        Self::create_from_repo(name, repo, path)?
      }
    };

    this.add_to_parent_config(parent_config)?;
    this.add_to_parent_gitignore(parent_root)?;
    this.ensure_metadata(parent_root)?;

    Ok(this)
  }

  /// Create a project from an existing repo. `path` must be the relative path
  /// from the parent repo to the project repo.
  fn create_from_repo(name: &str, repo: Repository, path: &Path) -> Result<Self> {
    let url = find_default_url(&repo)?.context("Failed to find a remote url to use")?;

    Ok(Self {
      name: name.to_owned(),
      path: path.to_owned(),
      url,
      repo,
    })
  }

  /// Clones a project. `path` is relative from the parent repo to the project
  /// repo.
  fn clone_project(name: &str, path: &Path, url: &str) -> Result<Self> {
    // make sure full path exists
    match fs::create_dir_all(path) {
      Ok(_) => (),
      Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
      Err(e) => return Err(e.into()),
    }

    let mut builder = RepoBuilder::new();
    let mut fetch_opts = FetchOptions::new();
    let mut remote_cbs = RemoteCallbacks::new();
    remote_cbs.credentials(get_credentials_cb());
    fetch_opts.remote_callbacks(remote_cbs);
    builder.fetch_options(fetch_opts);

    let repo = builder
      .clone(url, path)
      .context("Failed to clone repo when creating project")?;

    Ok(Self {
      name: name.to_owned(),
      path: path.to_owned(),
      url: url.to_owned(),
      repo,
    })
  }

  /// Unlinks this project from the parent. This does not delete the project dir
  /// from the filesystem. It clears all metadata associated with the project.
  pub fn unlink(&self, parent_root: &Path, parent_config: &Path) -> Result<()> {
    self.remove_from_parent_config(parent_config)?;
    self.remove_from_gitignore(parent_root)?;
    self.clear_metadata()?;
    Ok(())
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn url(&self) -> &str {
    &self.url
  }

  pub fn repo(&self) -> &Repository {
    &self.repo
  }

  /// Adds this project to the parent's config file
  fn add_to_parent_config(&self, parent_config: &Path) -> Result<()> {
    let path = parent_config;

    // ensure parent dirs exist
    if let Some(parent) = path.parent() {
      match fs::create_dir_all(parent) {
        Ok(_) => (),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
      };
    }

    // open or create file in rw mode
    // TODO: atomic write
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

    doc["projects"][&self.name]["url"] = value(&self.url);
    doc["projects"][&self.name]["path"] = value(
      self
        .path
        .to_str()
        .context("Project path must be valid utf-8")?,
    );

    let toml = doc.to_string();
    let toml = toml.as_bytes();

    file.seek(SeekFrom::Start(0))?;
    file.set_len(toml.len() as u64)?;
    file.write_all(toml)?;
    file.unlock()?;

    Ok(())
  }

  /// Removes this project's entry from the parent config file
  fn remove_from_parent_config(&self, parent_config: &Path) -> Result<String> {
    // TODO: atomic write
    let mut file = OpenOptions::new()
      .read(true)
      .write(true)
      .create(false)
      .truncate(false)
      .open(parent_config)?;

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

    Ok(path_string)
  }

  /// Writes the project metadata to the project repo. `parent_root` must be an
  /// absolute path.
  pub fn ensure_metadata(&self, parent_root: &Path) -> Result<()> {
    let file = self.repo.path().join(Self::METADATA_FILE);
    println!(
      "Writing path \"{}\" to metadata file \"{}\"",
      parent_root.to_str().unwrap(),
      file.to_str().unwrap()
    );
    fs::write(
      file,
      parent_root
        .to_str()
        .context("Parent path must be valid utf-8")?,
    )?;
    Ok(())
  }

  fn clear_metadata(&self) -> Result<()> {
    let file = self.repo.path().join(Self::METADATA_FILE);
    if file.exists() {
      fs::remove_file(file)?;
    }
    Ok(())
  }

  /// Adds the project dir to the parent's gitignore so git doesn't track it.
  fn add_to_parent_gitignore(&self, parent_root: &Path) -> Result<()> {
    let path_string = self
      .path
      .to_str()
      .context("Project path must be valid utf-8")?;

    // TODO: atomic write
    let mut file = OpenOptions::new()
      .read(true)
      .write(true)
      .create(true)
      .truncate(false)
      .open(parent_root.join(".gitignore"))?;
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

    Ok(())
  }

  fn remove_from_gitignore(&self, parent_root: &Path) -> Result<()> {
    let path_string = self
      .path
      .to_str()
      .context("Project path must be valid utf-8")?;

    let ignore_path = parent_root.join(".gitignore");
    if !ignore_path.exists() {
      return Ok(());
    }

    // TODO: atomic write
    let mut file = OpenOptions::new()
      .read(true)
      .write(true)
      .open(&ignore_path)?;
    file.lock()?;

    let mut ignore = String::new();
    file.read_to_string(&mut ignore)?;

    let mut new_ignore = ignore
      .lines()
      .filter(|line| *line != path_string)
      .collect::<Vec<_>>()
      .join("\n");
    new_ignore.push('\n');

    file.seek(SeekFrom::Start(0))?;
    file.set_len(new_ignore.len() as u64)?;
    file.write_all(new_ignore.as_bytes())?;
    file.unlock()?;

    Ok(())
  }

  /// Loads this project's config
  pub fn load_project_config(&self) -> Result<ProjectConfig> {
    project_config::load_with_path(&self.path, &self.repo)
  }

  /// Loads this project's user config
  pub fn load_user_config<'config>(&'config self) -> Result<UserConfig<'config>> {
    UserConfig::new(&self.repo)
  }
}

/// Gets the url of the first remote on the repo
fn find_default_url(repo: &Repository) -> Result<Option<String>> {
  let names = repo.remotes()?;
  let Some(name) = names.iter().next() else {
    return Ok(None);
  };

  let name = name?.context("Remote names must be valid utf-8")?;
  let remote = repo.find_remote(name)?;
  Ok(Some(remote.url()?.to_string()))
}
