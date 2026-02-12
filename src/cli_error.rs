#[derive(Debug)]
/// Enumeration of all error types, mapped to a nonzero return code
pub enum CliError {
  /// Generic/unknown error
  Generic = 1,
  BadBranchName,
  GitProcFailed,
}

/// Treat any io error as a git process failure
impl From<std::io::Error> for CliError {
  fn from(_value: std::io::Error) -> Self {
    CliError::GitProcFailed
  }
}

pub type CliResult = Result<(), CliError>;
