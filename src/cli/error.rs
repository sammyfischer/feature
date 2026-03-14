use std::string::FromUtf8Error;

#[derive(Debug)]
#[repr(u8)]
/// Enumeration of all error types, mapped to a nonzero return code
pub enum CliError {
  /// Generic/unknown error
  Generic(String) = 1,

  /// A process that was spawned failed to complete or returned an error
  SubprocessFailed(String),

  /// An error with the config file
  Config(String),
}

impl std::fmt::Display for CliError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CliError::Generic(msg) => write!(f, "{}", msg),
      CliError::SubprocessFailed(msg) => write!(f, "{}", msg),
      CliError::Config(msg) => write!(f, "{}", msg),
    }
  }
}

// Convert io errors
impl From<std::io::Error> for CliError {
  fn from(value: std::io::Error) -> Self {
    CliError::Generic(format!("{}", value))
  }
}

impl From<FromUtf8Error> for CliError {
  fn from(value: FromUtf8Error) -> Self {
    CliError::SubprocessFailed(format!("{}", value))
  }
}

// toml serialization error
impl From<toml::ser::Error> for CliError {
  fn from(value: toml::ser::Error) -> Self {
    CliError::Config(format!("{}", value))
  }
}

// toml deserialization error
impl From<toml::de::Error> for CliError {
  fn from(value: toml::de::Error) -> Self {
    CliError::Config(format!("{}", value))
  }
}

impl From<toml_edit::TomlError> for CliError {
  fn from(value: toml_edit::TomlError) -> Self {
    CliError::Config(format!("{}", value))
  }
}

impl From<figment::Error> for CliError {
  fn from(value: figment::Error) -> Self {
    CliError::Config(format!("{}", value))
  }
}
