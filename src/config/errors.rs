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
