use std::cmp::Reverse;
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

/// Gets a list of all version tags from the repo. Set `sort` to true to
/// automatically sort by date.
pub fn get_version_tags(
  repo: &Repository,
  project_config: &ProjectConfig,
  sort: bool,
) -> Result<Vec<VersionTag>> {
  let names = repo.tag_names(Some(&project_config.version.pattern))?;

  let mut versions = Vec::new();
  for name in &names {
    let name = name?.context("Tag names must be utf-8")?;
    let rf = repo.resolve_reference_from_short_name(name)?;
    let commit = rf.peel_to_commit()?;

    // save tag with the commit time for sorting
    versions.push((VersionTag::new(name, commit.id()), commit.time()));
  }

  if sort {
    // sort by time (recent first), and unwrap the VersionTag (discarding the time)
    versions.sort_by_key(|it| Reverse(it.1));
  }
  Ok(versions.into_iter().map(|(version, _)| version).collect())
}

/// Find the current version tag for the given commit. This walks up the commit
/// history from the commit until it finds a tag or history ends.
///
/// # Returns
/// The version tag, along with the number of commits introduced since that
/// version. If no tag is found, returns None.
pub fn find_current_version(
  repo: &Repository,
  project_config: &ProjectConfig,
  commit: Oid,
) -> Result<Option<(VersionTag, usize)>> {
  // don't need to sort
  let tags = get_version_tags(repo, project_config, false)?;

  let lookup_tag = tags
    .iter()
    .map(|tag| (tag.commit, tag))
    .collect::<HashMap<Oid, &VersionTag>>();

  let mut walk = repo.revwalk()?;
  walk.push(commit)?;

  let mut closest = None;
  for (i, id) in walk.flatten().enumerate() {
    if let Some(tag) = lookup_tag.get(&id) {
      closest = Some(((**tag).clone(), i));
      break;
    }
  }

  Ok(closest)
}

/// Finds the previous version and number of commits since. This is like
/// [find_current_version], but it begins the graph traversal from the commits
/// first parent.
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

  Ok(Some(prev))
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
