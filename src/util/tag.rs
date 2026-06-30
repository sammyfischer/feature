use std::fmt::Display;

use anyhow::{Context, Result, anyhow};
use git2::{Oid, Reference, Repository};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

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

/// Find the current semver tag for the given reference. This does an
/// ahead/behind graph calculation against each semver tag in parallel.
pub fn find_current_semver(repo: &Repository, reference: &Reference) -> Result<Option<SemverTag>> {
  let upstream = reference.peel_to_commit()?.id();

  let mut tags = get_semver_tags(repo)?;
  // ascending order, e.g. v1.0.0 -> v1.0.1 -> v2.0.0
  tags.sort();

  let repo_dir = repo.path().to_owned();
  let work_dir = repo.workdir().to_owned();

  // perform graph traversals in parallel, since there could be many tags and
  // it's a readonly operation
  let ancestors: Vec<_> = tags
    .par_iter()
    .map(|tag| -> Result<Option<(SemverTag, usize)>> {
      let repo = match &work_dir {
        Some(work_dir) => {
          let repo = Repository::open_bare(&repo_dir)?;
          repo.set_workdir(work_dir, false)?;
          repo
        }
        None => Repository::open(&repo_dir)?,
      };

      let (ahead, behind) = repo.graph_ahead_behind(tag.commit, upstream)?;

      // ancestors only
      if ahead > 0 {
        return Ok(None);
      }

      Ok(Some((tag.to_owned(), behind)))
    })
    .collect();

  // pair of the tag and its distance from `upstream`
  let mut closest_ancestor = (None, None);

  for tag in ancestors {
    let tag = match tag {
      Ok(it) => it,
      Err(e) => return Err(anyhow!(e)),
    };

    let Some((tag, distance)) = tag else {
      continue;
    };

    // because it's ascending order (by version) and we want the most recent
    // version, if two version tags point to the same commit, we need to
    // overwrite the previous one using a <= check
    if closest_ancestor.1.is_none_or(|it| distance <= it) {
      closest_ancestor = (Some(tag), Some(distance));
    }
  }

  Ok(closest_ancestor.0)
}
