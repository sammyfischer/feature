use std::collections::HashMap;
use std::fmt::Display;

use anyhow::{Context, Result};
use git2::{Oid, Repository};

use crate::core::NotFoundExt;
use crate::core::project_config::ProjectConfig;

/// An existing version tag in the repo
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionTag {
  name: String,
  commit: Oid,
}

impl VersionTag {
  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn commit(&self) -> Oid {
    self.commit
  }

  /// Name can be of the format "v*.*.*" or "*.*.*"
  pub fn new(name: &str, commit: Oid) -> Self {
    Self {
      name: name.to_string(),
      commit,
    }
  }
}

impl Display for VersionTag {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.name())
  }
}

/// Gets a list of all version tags from the repo
pub fn get_version_tags(
  repo: &Repository,
  project_config: &ProjectConfig,
) -> Result<Vec<VersionTag>> {
  let names = repo.tag_names(Some(&project_config.version.pattern))?;

  let mut versions = Vec::new();
  for name in &names {
    let name = name?.context("Tag names must be utf-8")?;
    let tag = repo.resolve_reference_from_short_name(name)?;
    let commit = tag.peel_to_commit()?.id();

    versions.push(VersionTag::new(name, commit));
  }

  Ok(versions)
}

/// Find the current version tag for the given commit. This walks up the commit
/// history from the commit until it finds a tag or history ends.
pub fn find_current_version(
  repo: &Repository,
  project_config: &ProjectConfig,
  commit: Oid,
) -> Result<Option<VersionTag>> {
  let tags = get_version_tags(repo, project_config)?;
  let lookup_tag = tags
    .iter()
    .map(|tag| (tag.commit, tag))
    .collect::<HashMap<Oid, &VersionTag>>();

  let mut walk = repo.revwalk()?;
  walk.push(commit)?;

  let mut closest = None;
  for id in walk.flatten() {
    if let Some(tag) = lookup_tag.get(&id) {
      closest = Some((**tag).clone());
      break;
    }
  }

  Ok(closest)
}

/// Get the name of the previous version and number of commits since
pub fn since_prev_version(
  repo: &Repository,
  project_config: &ProjectConfig,
  current: &VersionTag,
) -> Result<Option<(VersionTag, usize)>> {
  let commit = repo.find_commit(current.commit)?;

  // start from commit before the tag
  let upstream = match commit.parent(0).not_found_ok()? {
    Some(it) => it.id(),
    None => return Ok(None),
  };

  let Some(prev) = find_current_version(repo, project_config, upstream)? else {
    return Ok(None);
  };

  let (since, _) = repo.graph_ahead_behind(current.commit, prev.commit)?;
  Ok(Some((prev, since)))
}

/// Checks if the given tag name matches the configured version pattern
pub fn is_version_tag(
  repo: &Repository,
  project_config: &ProjectConfig,
  tag: &str,
) -> Result<bool> {
  let names = repo.tag_names(Some(&project_config.version.pattern))?;

  for other in names.iter().flatten() {
    if other.is_some_and(|other| tag == other) {
      return Ok(true);
    }
  }

  Ok(false)
}
