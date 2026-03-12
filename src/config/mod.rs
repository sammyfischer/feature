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

/// Reads and deserializes the config file
pub fn read() -> ConfigResult<Config> {
  if !Path::new(FILENAME).exists() {
    return Ok(Config::default());
  };

  let text = fs::read_to_string(FILENAME)?;
  let config = toml::from_str(&text)?;

  Ok(config)
}

/// Reads the config file and loads a mutable config document
pub fn read_doc() -> ConfigResult<DocumentMut> {
  // if the file doesn't exist, return an empty document
  if !Path::new(FILENAME).exists() {
    return Ok(DocumentMut::new());
  };

  let text = fs::read_to_string(FILENAME)?;
  let doc = text.parse::<DocumentMut>()?;

  Ok(doc)
}

/// Writes back the config document. Creates the file if it doesn't exist
pub fn write(doc: &DocumentMut) -> ConfigResult {
  let text = doc.to_string();
  fs::write(FILENAME, text)?;
  Ok(())
}
