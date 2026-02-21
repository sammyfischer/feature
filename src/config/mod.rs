use std::fs::{self, File};
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::cli::def::ConfigSetArgs;
use crate::config::errors::ConfigError;

pub mod errors;

const FILENAME: &str = ".feature.toml";

pub type ConfigResult<T = ()> = Result<T, ConfigError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
  /// List of protected branches
  pub protected_branches: Vec<String>,

  /// Use interactive mode by default for supported commands
  pub interactive: bool,

  /// Pager to use to view in interactive mode
  pub pager: String,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      protected_branches: vec!["main".to_string(), "master".to_string()],
      interactive: false,
      pager: "less".to_string(),
    }
  }
}

impl Config {
  pub fn set(&mut self, args: &ConfigSetArgs) -> ConfigResult {
    if let Some(protected_branches) = &args.protected_branches {
      self.protected_branches = protected_branches.clone();
    };

    if let Some(interactive) = args.interactive {
      self.interactive = interactive;
    };

    if let Some(pager) = &args.pager {
      self.pager = pager.clone();
    };

    Ok(())
  }
}

/// Reads and deserializes the config file
pub fn read_config() -> ConfigResult<Config> {
  let text = fs::read_to_string(FILENAME)
    .map_err(|_| ConfigError::Io("Couldn't read file contents".to_string()))?;

  let config = toml::from_str(&text)
    .map_err(|_| ConfigError::Serialize("Couldn't parse config file".to_string()))?;

  Ok(config)
}

/// Serializes and overwrites the config file
pub fn write_config(config: &Config) -> ConfigResult {
  // get serialized config string
  let text = toml::to_string_pretty(&config)
    .map_err(|_| ConfigError::Serialize("Couldn't seralize config".to_string()))?;

  // open or create file
  let mut file = File::create(FILENAME)
    .map_err(|_| ConfigError::Io("Couldn't create/open config file".to_string()))?;

  // grab lock
  file
    .lock()
    .map_err(|_| ConfigError::Io("Couldn't acquire config file lock".to_string()))?;

  // write config
  file
    .write(text.as_bytes())
    .map_err(|_| ConfigError::Io("Couldn't write config to config file".to_string()))?;

  Ok(())
}
