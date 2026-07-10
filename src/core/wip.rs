use anyhow::{Context, Result, anyhow};
use console::style;
use git2::Repository;

use crate::core::advice::NOT_ON_BRANCH_MSG;
use crate::core::branch_info::BranchInfo;

/// The base ref namespace for feature wips. Doesn't include a trailing slash.
pub const WIP_NAMESPACE: &str = "refs/feature/wips";

/// Gets the refname the branch's wips. `branch_name` should be the shortname of
/// a local branch or a glob pattern.
pub fn get_wip_refname(branch_name: &str) -> String {
  format!("{}/{}", WIP_NAMESPACE, branch_name)
}

/// Parses an optional stash spec. Fills in defaults and returns the branch and
/// stash index.
pub fn parse_wip_spec(repo: &Repository, spec: Option<&str>) -> Result<(BranchInfo, usize)> {
  let (name, num): (Option<String>, Option<usize>) = if let Some(spec) = spec {
    if spec.contains(':') {
      // contains ':', must be name:index
      let (branch, index) = spec
        .split_once(':')
        .context("Invalid format for wip spec")?;

      (Some(branch.to_string()), Some(index.parse()?))
    } else if spec.starts_with(|c: char| c.is_numeric()) {
      // starts with number, must be index
      (None, Some(spec.parse()?))
    } else {
      // must be name
      (Some(spec.to_owned()), None)
    }
  } else {
    (None, None)
  };

  // name defaults to current branch, index defaults to 0
  let (branch, num) = (
    match name {
      Some(name) => BranchInfo::from_name_dwim(repo, &name)?
        .with_context(|| format!("Failed to find branch: {}", &name))?,
      None => {
        let head = repo.head()?;
        if !head.is_branch() {
          return Err(anyhow!(NOT_ON_BRANCH_MSG));
        }
        BranchInfo::from_reference(&head.resolve()?)?
      }
    },
    num.unwrap_or(0),
  );

  Ok((branch, num))
}

/// Displays a wip spec with colors
pub fn display_wip_spec(name: &str, num: usize) -> String {
  format!(
    "{}{}{}",
    style(name).cyan(),
    style(":").dim(),
    style(num).cyan()
  )
}
