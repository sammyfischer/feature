#[derive(Debug)]
pub enum ConfigError {
  Serialize(String),
  Io(String),
}

impl std::fmt::Display for ConfigError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", match self {
      ConfigError::Serialize(msg) => msg,
      ConfigError::Io(msg) => msg,
    })
  }
}

impl From<std::io::Error> for ConfigError {
  fn from(value: std::io::Error) -> Self {
    ConfigError::Io(format!("{}", value))
  }
}

// toml deserialization error
impl From<toml::de::Error> for ConfigError {
  fn from(value: toml::de::Error) -> Self {
    ConfigError::Serialize(format!("{}", value))
  }
}

// toml_edit error
impl From<toml_edit::TomlError> for ConfigError {
  fn from(value: toml_edit::TomlError) -> Self {
    ConfigError::Serialize(format!("{}", value))
  }
}
