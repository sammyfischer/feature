use std::fmt::Display;

use anyhow::{Context, Result};
use git2::{Oid, Reference, Repository};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemverTag {
  pub commit: Oid,
  pub major: u32,
  pub minor: u32,
  pub patch: u32,
}

impl SemverTag {
  pub fn name(&self) -> String {
    format!("v{}.{}.{}", self.major, self.minor, self.patch)
  }

  /// Name must be of the format "v*.*.*"
  pub fn new(name: &str, commit: Oid) -> Result<Self> {
    assert!(name.starts_with('v'), "Invalid format for semver tag");

    let rest = &name[1..];
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

    Ok(Self {
      commit,
      major,
      minor,
      patch,
    })
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

/// Find the current semver tag for the given reference. This does an
/// ahead/behind graph calculation against each semver tag.
pub fn find_current_semver(repo: &Repository, reference: &Reference) -> Result<Option<SemverTag>> {
  let upstream = reference.peel_to_commit()?.id();

  let mut tags = get_semver_tags(repo)?;
  // ascending order, e.g. v1.0.0 -> v1.0.1 -> v2.0.0
  tags.sort();

  // pair of the tag and its distance from `upstream`
  let mut closest_ancestor = (None, None);
  for tag in tags {
    let (ahead, behind) = repo.graph_ahead_behind(tag.commit, upstream)?;
    // ancestors only
    if ahead > 0 {
      continue;
    }

    // because it's ascending order and we want the most recent version, if two
    // version tags point to the same commit, we need to overwrite the previous
    // one using a <= check
    if closest_ancestor.1.is_none_or(|it| behind <= it) {
      closest_ancestor = (Some(tag), Some(behind));
    }
  }

  Ok(closest_ancestor.0)
}
