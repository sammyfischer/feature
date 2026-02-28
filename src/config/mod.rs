use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

use crate::config::errors::ConfigError;

pub mod errors;

const FILENAME: &str = ".feature.toml";

pub type ConfigResult<T = ()> = Result<T, ConfigError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
  /// List of protected branches
  pub protected_branches: Vec<String>,

  /// Separator used between words in branch names
  pub branch_sep: String,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      protected_branches: vec!["main".to_string(), "master".to_string()],
      branch_sep: "-".to_string(),
    }
  }
}

/// Reads and deserializes the config file
pub fn read() -> ConfigResult<Config> {
  if !Path::new(FILENAME).exists() {
    return Ok(Config::default());
  };

  let text = fs::read_to_string(FILENAME)
    .map_err(|_| ConfigError::Io("Couldn't read file contents".to_string()))?;

  let config = toml::from_str(&text)
    .map_err(|_| ConfigError::Serialize("Couldn't parse config file".to_string()))?;

  Ok(config)
}

/// Reads the config file and loads a mutable config document
pub fn read_doc() -> ConfigResult<DocumentMut> {
  if !Path::new(FILENAME).exists() {
    return Ok(DocumentMut::new());
  };

  let text = fs::read_to_string(FILENAME)
    .map_err(|_| ConfigError::Io("Couldn't read file contents".to_string()))?;

  let doc = text
    .parse::<DocumentMut>()
    .map_err(|_| ConfigError::Serialize("Couldn't parse config file".to_string()))?;

  Ok(doc)
}

/// Writes back the config document
pub fn write(doc: &DocumentMut) -> ConfigResult {
  let text = doc.to_string();

  fs::write(FILENAME, text)
    .map_err(|_| ConfigError::Io("Couldn't write config to file".to_string()))?;

  Ok(())
}
