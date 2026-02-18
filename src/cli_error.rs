#[derive(Debug)]
/// Enumeration of all error types, mapped to a nonzero return code
pub enum CliError {
  /// Generic/unknown error
  Generic = 1,
  BadBranchName,
  /// A process that was spawned failed to complete or returned an error
  SubprocessFailed,
}

// Convert io errors
impl From<std::io::Error> for CliError {
  fn from(_value: std::io::Error) -> Self {
    CliError::Generic
  }
}

pub type CliResult<T = ()> = Result<T, CliError>;
