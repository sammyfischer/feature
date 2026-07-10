use std::collections::HashMap;
use std::fmt::Display;

use anyhow::{Context, Result};
use git2::{Commit, Oid, Repository};

/// A real tag on the repo of the format "v.*.*.*"
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SemverTag {
  pub commit: Oid,
  pub major: u32,
  pub minor: u32,
  pub patch: u32,
}

impl PartialOrd for SemverTag {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for SemverTag {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    match self.major.cmp(&other.major) {
      core::cmp::Ordering::Equal => {}
      ord => return ord,
    }
    match self.minor.cmp(&other.minor) {
      core::cmp::Ordering::Equal => {}
      ord => return ord,
    }
    self.patch.cmp(&other.patch)
  }
}

impl SemverTag {
  pub fn name(&self) -> String {
    format!("v{}.{}.{}", self.major, self.minor, self.patch)
  }

  /// Name can be of the format "v*.*.*" or "*.*.*"
  pub fn new(name: &str, commit: Oid) -> Result<Self> {
    let (major, minor, patch) = Self::parse(name)?;
    Ok(Self {
      commit,
      major,
      minor,
      patch,
    })
  }

  /// Parses out the major, minor, and patch version of a semver string. The
  /// leading `v` in the version string is optional.
  pub fn parse(version: &str) -> Result<(u32, u32, u32)> {
    let rest = if let Some(stripped) = version.strip_prefix('v') {
      stripped
    } else {
      version
    };
    let mut parts = rest.split('.');

    let major = parts
      .next()
      .context("Failed to get major version from semver tag")?
      .parse::<u32>()
      .context("Invalid format for major version")?;

    let minor = parts
      .next()
      .context("Failed to get minor version from semver tag")?
      .parse::<u32>()
      .context("Invalid format for minor version")?;

    let patch = parts
      .next()
      .context("Failed to get patch version from semver tag")?
      .parse::<u32>()
      .context("Invalid format for patch version")?;

    Ok((major, minor, patch))
  }
}

impl Display for SemverTag {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.name())
  }
}

/// Gets a list of all semver tags from the repo
pub fn get_semver_tags(repo: &Repository) -> Result<Vec<SemverTag>> {
  let names = repo.tag_names(Some("v*.*.*"))?;

  let mut semvers = Vec::new();
  for name in &names {
    let name = name?.context("Tag names must be utf-8")?;
    let tag = repo.resolve_reference_from_short_name(name)?;
    let commit = tag.peel_to_commit()?.id();

    semvers.push(SemverTag::new(name, commit)?);
  }

  Ok(semvers)
}

/// Find the current semver tag for the given commit. This walks up the commit
/// history from the commit until it finds a tag or history ends.
pub fn find_current_semver(repo: &Repository, commit: &Commit) -> Result<Option<SemverTag>> {
  let upstream = commit.id();

  let tags = get_semver_tags(repo)?;
  let lookup_tag = tags
    .iter()
    .map(|tag| (tag.commit, tag))
    .collect::<HashMap<Oid, &SemverTag>>();

  let mut walk = repo.revwalk()?;
  walk.push(upstream)?;
  walk.simplify_first_parent()?;

  let mut closest = None;
  for id in walk {
    let id = id?;

    if let Some(tag) = lookup_tag.get(&id) {
      closest = Some(**tag);
      break;
    }
  }

  Ok(closest)
}
