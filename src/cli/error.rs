use std::string::FromUtf8Error;

/// Returns a `CliError` with a format string.
///
/// The first arg must be a name in the `CliError` namespace. All remaining args are
/// passed to `format!` as-is.
#[macro_export]
macro_rules! cli_err {
  ($kind:ident, $($format_args:tt)*) => {
    $crate::cli::error::CliError::$kind(format!($($format_args)*))
  };
}

/// Like `cli_err!` but returns a closure. Ideal for passing into `map_err`.
///
/// The first arg must be a name in the `CliError` namespace. The next arg is the name of the error
/// variable passed into the closure, which can be used in the format string. All remaining args are
/// passed to `format!` as-is.
#[macro_export]
macro_rules! cli_err_fn {
  ($kind:ident, $err:ident, $($format_args:tt)*) => {
    { |$err| $crate::cli::error::CliError::$kind(format!($($format_args)*)) }
  };
}

#[derive(Debug)]
#[repr(u8)]
/// Enumeration of all error types, mapped to a nonzero return code
pub enum CliError {
  /// Generic/unknown error
  Generic(String) = 1,

  /// A process that was spawned failed to complete or returned an error
  Process(String),

  /// An error with the config file
  Config(String),

  /// An error with the database file
  Database(String),

  /// An error with libgit
  Git(String),

  /// Precommit hooks failed
  Precommit(String),

  /// Push safety checks failed
  Push(String),
}

impl std::fmt::Display for CliError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CliError::Generic(msg) => write!(f, "{msg}"),
      CliError::Process(msg) => write!(f, "{msg}"),
      CliError::Config(msg) => write!(f, "{msg}"),
      CliError::Database(msg) => write!(f, "{msg}"),
      CliError::Git(msg) => write!(f, "{msg}"),
      CliError::Precommit(msg) => write!(f, "{msg}"),
      CliError::Push(msg) => write!(f, "{msg}"),
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
    CliError::Process(format!("{}", value))
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

impl From<git2::Error> for CliError {
  fn from(value: git2::Error) -> Self {
    CliError::Git(format!("{}", value))
  }
}
