use anyhow::Result;
use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
  /// Main remote name
  pub default_remote: String,

  /// List of possible base branches
  pub bases: Vec<String>,

  /// List of branches to protect from pushes/deletion. By default, base branches are already
  /// protected and don't need to be added
  pub protect: Vec<String>,

  /// Separator used between words in branch names
  pub branch_sep: String,

  /// Template for creating branch names. See `feature start --help` for more info
  pub branch_format: String,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      default_remote: "origin".into(),
      bases: vec!["main".into()],
      protect: vec![],
      branch_sep: "-".into(),
      branch_format: "%s".into(),
    }
  }
}

/// Loads and layers config from all sources except cli options
pub fn load() -> Result<Config> {
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

  let config: Config = figment.extract()?;
  Ok(config)
}

pub mod project {
  use std::fs;
  use std::path::PathBuf;

  use anyhow::Result;
  use toml_edit::DocumentMut;

  use crate::config::Config;

  pub fn path() -> PathBuf {
    PathBuf::from("feature.toml")
  }

  /// Reads the config file and loads a mutable config document
  pub fn load_doc() -> Result<DocumentMut> {
    let path = self::path();
    // if the file doesn't exist, return an empty document
    if !path.exists() {
      return Ok(DocumentMut::new());
    };

    let text = fs::read_to_string(path)?;
    let doc = text.parse::<DocumentMut>()?;

    Ok(doc)
  }

  pub fn save(doc: DocumentMut) -> Result<()> {
    let path = self::path();
    let text = doc.to_string();

    fs::write(&path, text)?;
    println!("Wrote to config file at {}", &path.to_string_lossy());
    Ok(())
  }

  /// Saves an entire default config to the project directory
  pub fn save_default() -> Result<()> {
    let path = self::path();
    let config = Config::default();
    let toml_raw = toml::to_string_pretty(&config)?;

    fs::write(&path, toml_raw)?;
    println!("Created default config file at {}", &path.to_string_lossy());
    Ok(())
  }
}

pub mod user {
  use std::fs;
  use std::io::ErrorKind;
  use std::path::PathBuf;

  use anyhow::{Result, anyhow};
  use toml_edit::DocumentMut;

  use crate::config::Config;

  /// Returns the config file located in the platform's standard config directory
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

  /// Reads the config file and loads a mutable config document
  pub fn load_doc() -> Result<DocumentMut> {
    let path = self::path()?;

    // if the file doesn't exist, return an empty document
    if !path.exists() {
      return Ok(DocumentMut::new());
    };

    let text = fs::read_to_string(path)?;
    let doc = text.parse::<DocumentMut>()?;

    Ok(doc)
  }

  pub fn save(doc: DocumentMut) -> Result<()> {
    let path = self::ensure_path()?;
    let text = doc.to_string();

    fs::write(&path, text)?;
    println!("Wrote to config file at {}", &path.to_string_lossy());
    Ok(())
  }

  /// Saves an entire default config to the user config directory
  pub fn save_default() -> Result<()> {
    let path = self::ensure_path()?;
    let config = Config::default();
    let toml_raw = toml::to_string_pretty(&config)?;

    fs::write(&path, toml_raw)?;
    println!("Created default config file at {}", &path.to_string_lossy());
    Ok(())
  }
}
