use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const FILENAME: &str = ".feature.toml";

pub type ConfigResult<T = ()> = Result<T, ConfigError>;

#[derive(Debug)]
pub enum ConfigError {
  Serialize,
  Deserialize,
  Read,
  Write,
}

impl std::fmt::Display for ConfigError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", match self {
      ConfigError::Serialize => "Error serializing config file",
      ConfigError::Deserialize => "Error deserializing config file",
      ConfigError::Read => "Error reading config file",
      ConfigError::Write => "Error writing config file",
    })
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
  /// List of protected branches
  protected_branches: Vec<String>,

  /// Use interactive mode by default for supported commands
  interactive: bool,
  /// Pager to use to view in interactive mode
  pager: String,
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
  pub fn set(&mut self, key: &str, value: &str) -> ConfigResult {
    let clean_key: &str = &key.replace("-", "_");
    match clean_key {
      "protected_branches" => {
        let branches = value.split(",");
        let mut clean_branches: Vec<String> = Vec::new();

        for b in branches {
          // consider validating branch names
          clean_branches.push(b.trim().to_string());
        }

        self.protected_branches = clean_branches;
      }

      "interactive" => {
        self.interactive = value.parse().map_err(|_| ConfigError::Serialize)?;
      }

      "pager" => {
        self.pager = value.to_string();
      }

      _ => {
        eprintln!("Unknown config key: {}", clean_key);
        return Err(ConfigError::Serialize);
      }
    };

    Ok(())
  }

  pub fn reset(&mut self, key: &str) -> ConfigResult {
    let clean_key: &str = &key.replace("-", "_");
    let default = Config::default();

    match clean_key {
      "protected_branches" => self.protected_branches = default.protected_branches,
      "interactive" => self.interactive = default.interactive,
      "pager" => self.pager = default.pager,
      _ => {
        eprintln!("Unknown config key: {}", clean_key);
        return Err(ConfigError::Serialize);
      }
    };

    Ok(())
  }
}

/// Reads and deserializes the config file
pub fn read_config() -> ConfigResult<Config> {
  let text = fs::read_to_string(FILENAME).map_err(|_| ConfigError::Read)?;
  let config = toml::from_str(&text).map_err(|_| ConfigError::Deserialize)?;
  Ok(config)
}

/// Serializes and overwrites the config file
pub fn write_config(config: &Config) -> ConfigResult {
  let text = toml::to_string_pretty(&config).map_err(|_| ConfigError::Serialize)?;
  fs::write(FILENAME, text).map_err(|_| ConfigError::Write)?;
  Ok(())
}

/// Creates an empty config file. Does nothing if file already exists
pub fn create_config() -> ConfigResult {
  let path = Path::new(FILENAME);
  if let Ok(true) = path.try_exists() {
    return Ok(());
  }
  fs::write(path, "").map_err(|_| ConfigError::Write)?;
  Ok(())
}
