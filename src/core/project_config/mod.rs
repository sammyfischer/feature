//! Representation of the cli config. Use [load] to get the entire flattened
//! config struct. Includes modules to work with specific config levels.

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use git2::Repository;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::project::Project;
use crate::core::project_config::branch::BranchConfig;
use crate::core::project_config::projects::ProjectsConfig;
use crate::core::project_config::version::VersionConfig;
use crate::core::user_config::UserConfig;

pub mod branch;
pub mod projects;
pub mod version;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ProjectConfig {
  /// Name of the remote to use when one can't be determined automatically
  pub default_remote: String,

  /// List of branches to protect from force-pushes/deletion
  pub protect: Vec<String>,

  /// Branch name options
  pub branch: BranchConfig,

  /// Version tag options
  pub version: VersionConfig,

  /// List of subprojects
  #[serde(default)]
  pub projects: ProjectsConfig,
}

impl Default for ProjectConfig {
  fn default() -> Self {
    Self {
      default_remote: "origin".into(),
      protect: vec!["main".into()],
      branch: Default::default(),
      version: Default::default(),
      projects: Default::default(),
    }
  }
}

#[derive(
  Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum PageWhen {
  #[default]
  Auto,
  Always,
  Never,
}

impl Display for PageWhen {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      PageWhen::Auto => write!(f, "auto"),
      PageWhen::Always => write!(f, "always"),
      PageWhen::Never => write!(f, "never"),
    }
  }
}

/// Loads a layered config, searching the default locations for each.
pub fn load_config(repo: &Repository) -> Result<ProjectConfig> {
  // check persistent config
  let config = UserConfig::new(repo)?;

  // default to standard location
  let path = config.config()?.unwrap_or_else(&local::path);
  load_with_path(&path, repo)
}

/// Loads a layered config, using the given path as the project-level config
/// file. If the repo has a parent project, that file will be layered and
/// resolved from the parent repo's git config. The global config file cannot be
/// changed.
pub fn load_with_path(project_config: &Path, repo: &Repository) -> Result<ProjectConfig> {
  // load defaults
  let mut figment = Figment::new().merge(Serialized::defaults(ProjectConfig::default()));

  // override with global config
  // ignore error, just don't load and move on
  if let Ok(path) = global::path() {
    figment = figment.merge(Toml::file(&path));
  }

  // override with parent config
  {
    let metadata_file = repo.path().join(Project::METADATA_FILE);
    if metadata_file.exists() {
      // resolve actual path
      let parent_root = PathBuf::from(fs::read_to_string(metadata_file)?);
      let parent = Repository::open(&parent_root)?;
      let parent_config = UserConfig::new(&parent)?
        .config()?
        .unwrap_or_else(|| parent_root.join(local::FILE));

      figment = figment.merge(Toml::file(&parent_config));
    }
  }

  // override with local config
  {
    let path = project_config;
    if path.exists() {
      figment = figment.merge(Toml::file(path));
    }
  }

  let config: ProjectConfig = figment.extract()?;
  Ok(config)
}

/// Generates the url of the schema file matching the version of this crate
#[inline]
fn get_schema_url() -> String {
  format!(
    "{}/raw/refs/tags/v{}/resources/config.schema.json",
    env!("CARGO_PKG_REPOSITORY"),
    env!("CARGO_PKG_VERSION")
  )
}

fn example_config() -> Result<String> {
  let mut config = ProjectConfig::default();

  // branch.template's default is None, but this is an example config so it
  // should have some value
  config.branch.template = Some("%s".to_string());

  Ok(toml::to_string_pretty(&config)?)
}

/// Functions to work the the local project config file
pub mod local {
  use std::fs::File;
  use std::io::Write;
  use std::path::PathBuf;

  use anyhow::Result;

  use crate::core::project_config::{example_config, get_schema_url};

  /// The name of the local config file
  pub const FILE: &str = "feature.toml";

  pub fn path() -> PathBuf {
    PathBuf::from(self::FILE)
  }

  /// Saves an entire default config to the project directory
  pub fn save_default() -> Result<()> {
    let path = self::path();
    let toml_raw = example_config()?;

    let mut file = File::create(&path)?;
    file.write_all(format!("\"$schema\" = \"{}\"\n\n", get_schema_url()).as_bytes())?;
    file.write_all(toml_raw.as_bytes())?;
    println!("Created default config file at {}", &path.to_string_lossy());
    Ok(())
  }
}

/// Functions to work the the global project config file
pub mod global {
  use std::fs::{self, File};
  use std::io::{ErrorKind, Write};
  use std::path::PathBuf;

  use anyhow::{Result, anyhow};

  use crate::core::project_config::{example_config, get_schema_url};

  /// Returns the config file located in the platform's standard config
  /// directory
  ///
  /// # Errors
  /// Returns an error if the config directory cannot be obtained.
  pub fn path() -> Result<PathBuf> {
    let mut path = dirs::config_dir().ok_or(anyhow!("Failed to find user config directory",))?;
    path.push("feature");
    path.push("config.toml");
    Ok(path)
  }

  /// Gets the path and ensure that all necessary directories are created
  fn ensure_path() -> Result<PathBuf> {
    let path = self::path()?;
    let Some(dir) = &path.parent() else {
      return Err(anyhow!("Failed to find parent directory of config file"));
    };

    // ensure full path exists
    match fs::create_dir_all(dir) {
      Ok(_) => Ok(()),
      Err(e) => match e.kind() {
        // ignore AlreadyExists error
        ErrorKind::AlreadyExists => Ok(()),

        _ => Err(e),
      },
    }?;

    Ok(path)
  }

  /// Saves an entire default config to the user's config directory
  pub fn save_default() -> Result<()> {
    let path = self::ensure_path()?;
    let toml_raw = example_config()?;

    let mut file = File::create(&path)?;
    file.write_all(format!("\"$schema\" = \"{}\"\n\n", get_schema_url()).as_bytes())?;
    file.write_all(toml_raw.as_bytes())?;
    println!("Created default config file at {}", &path.to_string_lossy());
    Ok(())
  }
}
