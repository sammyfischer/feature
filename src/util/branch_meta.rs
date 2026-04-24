use anyhow::{Result, anyhow};
use git2::{Branch, BranchType, ErrorCode, Reference, Repository};

use crate::lossy;

// In the future, this could keep a private cached value of the reference, and the resolve function
// would reuse that. The caller would have to invalidate the cahce after a fetch. This keeps the
// code clean but since fetches are rare, would reduce the number of calls to
// `Repository::find_reference()`.

/// Collected metadata for a branch
pub struct BranchMeta {
  refname: String,
  name: String,
  ty: BranchType,
}

impl BranchMeta {
  // ASSOCIATED FUNCTIONS

  /// The full refname of the branch, e.g. "refs/heads/main".
  #[inline]
  pub fn refname(&self) -> &str {
    &self.refname
  }

  /// The shorthand name of the branch, e.g. "main" for local, "origin/main" for remote
  #[inline]
  pub fn name(&self) -> &str {
    &self.name
  }

  /// The branch type (local or remote)
  #[inline]
  pub fn ty(&self) -> BranchType {
    self.ty
  }

  /// Resolves this to a symbolic [Reference]
  #[inline]
  pub fn resolve<'branch>(&self, repo: &'branch Repository) -> Result<Reference<'branch>> {
    Ok(repo.find_reference(&self.refname)?)
  }

  /// Resolves this branch and gets its upstream if it has one
  #[inline]
  pub fn upstream<'branch>(&self, repo: &'branch Repository) -> Result<Option<Branch<'branch>>> {
    match Branch::wrap(self.resolve(repo)?).upstream() {
      Ok(upstream) => Ok(Some(upstream)),
      Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  /// Whether this branch is a remote branch
  #[inline]
  pub fn is_remote(&self) -> bool {
    self.ty == BranchType::Remote
  }

  /// Given a remote branch, splits the name of the remote and the rest of the branch. Given a local
  /// branch, returns a clone of the name and None as the remote.
  ///
  /// # Returns
  ///
  /// A tuple where:
  /// - `0` is the shorter name of the branch
  /// - `1` is the name of the remote if this is actually an upstream branch
  pub fn split_name_and_remote(&self) -> Result<(String, Option<String>)> {
    Ok(if self.refname.starts_with("refs/remotes/") {
      let (remote, shorter_name) = self
        .name
        .split_once('/')
        .ok_or_else(|| anyhow!("Invalid format for remote branch: {}", self.name))?;

      (shorter_name.to_string(), Some(remote.to_string()))
    } else {
      (self.name.to_string(), None)
    })
  }

  // CONSTRUCTORS

  /// Creates a [BranchMeta] from a [Branch] (uses the [TryInto] impl)
  #[inline]
  pub fn from_branch<'branch>(branch: Branch<'branch>) -> Result<Self> {
    branch.try_into()
  }

  /// Creates a [BranchMeta] from a [Reference] (uses the [TryInto] impl)
  pub fn from_reference<'branch>(reference: Reference<'branch>) -> Result<Self> {
    let refname = lossy!(reference.name_bytes()).to_string();
    if !reference.is_branch() {
      return Err(anyhow!("Reference is not a branch: {}", refname));
    }
    let name = lossy!(reference.shorthand_bytes()).to_string();
    let ty = get_branch_type(&refname);

    Ok(Self { refname, name, ty })
  }

  /// Creates a [BranchMeta] from the refname of a branch. Needs a repository to search for the
  /// matching branch.
  pub fn from_refname(repo: &Repository, refname: &str) -> Result<Self> {
    let reference = repo.find_reference(refname)?;
    let name = lossy!(reference.shorthand_bytes()).to_string();
    let ty = get_branch_type(refname);

    Ok(Self {
      refname: refname.to_string(),
      name,
      ty,
    })
  }

  /// Creates a [BranchMeta] from (what is usually) user input
  #[inline]
  pub fn from_name_dwim(repo: &Repository, name: &str) -> Result<Option<Self>> {
    Ok(match repo.resolve_reference_from_short_name(name) {
      Ok(it) => Some(Self::from_reference(it)?),
      Err(e) if e.code() == ErrorCode::NotFound => None,
      Err(e) => return Err(e.into()),
    })
  }
}

impl<'branch> TryFrom<Branch<'branch>> for BranchMeta {
  type Error = anyhow::Error;

  fn try_from(value: Branch<'branch>) -> Result<Self> {
    let refname = lossy!(value.get().name_bytes()).to_string();
    let name = lossy!(value.get().shorthand_bytes()).to_string();
    let ty = get_branch_type(&refname);

    Ok(Self { refname, name, ty })
  }
}

#[inline]
fn get_branch_type(refname: &str) -> BranchType {
  if refname.starts_with("refs/remotes/") {
    BranchType::Remote
  } else {
    BranchType::Local
  }
}
