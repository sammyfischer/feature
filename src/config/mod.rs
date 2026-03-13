use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

use crate::cli::CliResult;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
  /// Main branch of the repository
  pub default_base: String,

  /// Main remote name
  pub default_remote: String,

  /// List of protected branches
  pub protected_branches: Vec<String>,

  /// Separator used between words in branch names
  pub branch_sep: String,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      default_base: "main".into(),
      default_remote: "origin".into(),
      protected_branches: vec!["main".into()],
      branch_sep: "-".into(),
    }
  }
}

/// Loads and layers config from all sources except cli options
pub fn load() -> CliResult<Config> {
  // load defaults
  let mut figment = Figment::new().merge(Serialized::defaults(Config::default()));

  // override with user config
  // ignore error, just don't load and move on
  if let Ok(path) = user::path() {
    figment = figment.merge(Toml::file(&path));
  }

  // override with project config
  {
    let path = project::path();
    if path.exists() {
      figment = figment.merge(Toml::file(path));
    }
  }

  let config = figment.extract::<Config>()?;
  Ok(config)
}

pub mod project {
  use std::fs;
  use std::path::PathBuf;

  use toml_edit::DocumentMut;

  use crate::cli::CliResult;

  pub fn path() -> PathBuf {
    PathBuf::from(".feature.toml")
  }

  /// Reads the config file and loads a mutable config document
  pub fn load_doc() -> CliResult<DocumentMut> {
    let path = self::path();
    // if the file doesn't exist, return an empty document
    if !path.exists() {
      return Ok(DocumentMut::new());
    };

    let text = fs::read_to_string(path)?;
    let doc = text.parse::<DocumentMut>()?;

    Ok(doc)
  }

  pub fn save(doc: DocumentMut) -> CliResult {
    let text = doc.to_string();
    fs::write(self::path(), text)?;
    Ok(())
  }
}

pub mod user {
  use std::fs;
  use std::io::ErrorKind;
  use std::path::PathBuf;

  use toml_edit::DocumentMut;

  use crate::cli::CliResult;
  use crate::cli::error::CliError;

  /// Returns the config file located in the platform's standard config directory
  /// # Errors
  /// Returns an error if the config directory cannot be obtained.
  pub fn path() -> CliResult<PathBuf> {
    let mut path = dirs::config_dir().ok_or(CliError::Config(
      "Couldn't find user config directory".into(),
    ))?;
    path.push("feature");
    path.push("config.toml");
    Ok(path)
  }

  /// Reads the config file and loads a mutable config document
  pub fn load_doc() -> CliResult<DocumentMut> {
    let path = self::path()?;

    // if the file doesn't exist, return an empty document
    if !path.exists() {
      return Ok(DocumentMut::new());
    };

    let text = fs::read_to_string(path)?;
    let doc = text.parse::<DocumentMut>()?;

    Ok(doc)
  }

  pub fn save(doc: DocumentMut) -> CliResult {
    let path = self::path()?;
    let Some(dir) = &path.parent() else {
      return Err(CliError::Config(
        "Failed to find parent directory of config".into(),
      ));
    };

    // ensure full path exists
    match fs::create_dir_all(dir) {
      Ok(_) => Ok(()),
      Err(e) => match e.kind() {
        // ignore AlreadyExists error
        ErrorKind::AlreadyExists => Ok(()),

        ErrorKind::PermissionDenied => Err(CliError::Config(format!(
          "Insufficient privilege to create directores in path: {}\n",
          &path.to_string_lossy()
        ))),

        _ => Err(CliError::Config(format!(
          "Failed to create path: {}. Error: {}",
          path.to_string_lossy(),
          e
        ))),
      },
    }?;

    let text = doc.to_string();
    fs::write(&path, text)?;
    println!("Wrote to config file at {}", &path.to_string_lossy());
    Ok(())
  }
}
