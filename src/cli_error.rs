#[derive(Debug)]
/// Enumeration of all error types, mapped to a nonzero return code
pub enum CliError {
  /// Generic/unknown error
  Generic = 1,
  BadBranchName,
  GitProcFailed,
}

// Convert io errors
impl From<std::io::Error> for CliError {
  fn from(_value: std::io::Error) -> Self {
    CliError::Generic
  }
}

pub type CliResult = Result<(), CliError>;
