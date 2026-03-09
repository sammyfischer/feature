use std::string::FromUtf8Error;

use crate::config::errors::ConfigError;

#[derive(Debug)]
#[repr(u8)]
/// Enumeration of all error types, mapped to a nonzero return code
pub enum CliError {
  /// Generic/unknown error
  Generic(String) = 1,

  /// A process that was spawned failed to complete or returned an error
  SubprocessFailed(String),

  /// An error with the config file
  ConfigError(ConfigError),
}

impl std::fmt::Display for CliError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CliError::Generic(msg) => write!(f, "{}", msg),
      CliError::SubprocessFailed(msg) => write!(f, "{}", msg),
      CliError::ConfigError(config_error) => write!(f, "{}", config_error),
    }
  }
}

// Convert io errors
impl From<std::io::Error> for CliError {
  fn from(value: std::io::Error) -> Self {
    CliError::Generic(format!("{}", value))
  }
}

impl From<ConfigError> for CliError {
  fn from(value: ConfigError) -> Self {
    CliError::ConfigError(value)
  }
}

impl From<FromUtf8Error> for CliError {
  fn from(value: FromUtf8Error) -> Self {
    CliError::SubprocessFailed(format!("{}", value))
  }
}
