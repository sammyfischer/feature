use std::ops::Range;

use anyhow::{Context, Result, anyhow};
use git2::build::CheckoutBuilder;
use git2::{
  BranchType,
  Commit,
  DiffOptions,
  Oid,
  Reference,
  Reflog,
  ReflogEntry,
  Repository,
  Time,
};

use crate::core::NotFoundExt;
use crate::core::string::TrimPrefix;

/// An existing wip in the repo. Wraps a [ReflogEntry].
pub struct Wip<'wip> {
  entry: ReflogEntry<'wip>,
  branch: String,
  index: usize,
}

impl<'wip> Wip<'wip> {
  fn from_reflog_entry(entry: ReflogEntry<'wip>, branch: String, index: usize) -> Self {
    Self {
      entry,
      branch,
      index,
    }
  }

  /// The branch this wip belongs to
  pub fn branch(&self) -> &str {
    &self.branch
  }

  /// The index in the wip list
  pub fn index(&self) -> usize {
    self.index
  }

  /// When the reflog was created
  pub fn time(&self) -> Time {
    self.entry.committer().when()
  }

  /// Reflog message
  pub fn message(&self) -> Result<String> {
    Ok(self.entry.message()?.unwrap_or_default().to_string())
  }

  /// Id of the commit containing the changes
  pub fn commit(&self) -> Oid {
    self.entry.id_new()
  }
}

/// A list of wips associated with a branch. Wraps a [Reflog].
pub struct WipList {
  /// The name of the wip's ref. This may not exist on disk.
  refname: String,

  /// The shorthand name of the local branch of this wip list.
  branch: String,

  /// The in-memory reflog of the wip's ref. If no ref exists, this will be an
  /// empty reflog.
  reflog: Reflog,
}

impl WipList {
  /// The base ref namespace for feature wips. Doesn't include a trailing slash.
  pub const NAMESPACE: &'static str = "refs/feature/wips";

  /// The base ref namespace for feature wips, including the trailing slash.
  /// Useful for trimming the prefix to get the branch name.
  const NAMESPACE_SLASH: &'static str = "refs/feature/wips/";

  /// Get a list from its ref
  pub fn from_reference(repo: &Repository, rf: &Reference) -> Result<Self> {
    let refname = rf.name()?.to_string();
    let branch = refname.trim_prefix_opt(Self::NAMESPACE_SLASH).to_string();
    let reflog = repo.reflog(&refname)?;
    Ok(Self {
      refname,
      branch,
      reflog,
    })
  }

  /// Get a list from the shorthand name of a branch
  pub fn from_branch(repo: &Repository, branch: String) -> Result<Self> {
    let refname = format!("{}/{}", Self::NAMESPACE, &branch);
    let reflog = repo.reflog(&refname)?;
    Ok(Self {
      refname,
      branch,
      reflog,
    })
  }

  /// Parses a wipspec and returns the list and the wip's index. Use
  /// [WipList::get] with the index to get the actual wip.
  pub fn parse_wipspec(repo: &Repository, spec: Option<&str>) -> Result<(Self, usize)> {
    let (name, index): (Option<String>, Option<usize>) = if let Some(spec) = spec {
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
    let (branch, index) = (
      match name {
        Some(name) => name,
        None => {
          let head = repo.head()?;
          if !head.is_branch() {
            return Err(anyhow!("Not on a branch"));
          }
          head.shorthand()?.to_string()
        }
      },
      index.unwrap_or(0),
    );

    let list = Self::from_branch(repo, branch)?;
    Ok((list, index))
  }

  /// Gets an iterator over the list
  pub fn iter(&self) -> WipIter<'_> {
    WipIter {
      range: (0..self.reflog.len()),
      list: self,
    }
  }

  /// Gets a particular wip from the list
  pub fn get(&self, index: usize) -> Option<Wip<'_>> {
    let entry = self.reflog.get(index)?;
    Some(Wip::from_reflog_entry(entry, self.branch.clone(), index))
  }

  /// Removes a wip from the list
  pub fn remove(&mut self, repo: &Repository, num: usize) -> Result<()> {
    self.reflog.remove(num, true)?;

    if self.reflog.is_empty() {
      let mut rf = repo.find_reference(&self.refname)?;
      rf.delete()?;
      return Ok(());
    }

    self.reflog.write()?;
    Ok(())
  }

  /// The branch of this wip list
  pub fn branch(&self) -> &str {
    &self.branch
  }

  /// Delete this entire wip list. This is safe to call if the underlying wip
  /// reference doesn't exist.
  pub fn delete(&mut self, repo: &Repository) -> Result<()> {
    if let Some(mut rf) = repo.find_reference(&self.refname).not_found_ok()? {
      rf.delete()?;
    }
    Ok(())
  }

  /// Push a new wip to the front of the list. Returns the newly created wip.
  ///
  /// # Params
  /// - `repo` - the repository
  /// - `msg` - the reflog message. Defaults to the parent commits message with
  ///   "WIP: " prepended
  /// - `staged` - push staged changes only, don't include unstaged
  /// - `untracked` - include untracked files (can be used with `staged = true`)
  /// - `keep` - keep changes in workdir after push
  pub fn push(
    &mut self,
    repo: &Repository,
    msg: Option<&str>,
    staged: bool,
    untracked: bool,
    keep: bool,
  ) -> Result<Wip<'_>> {
    let head = repo.head()?;
    let branch = repo.find_branch(&self.branch, BranchType::Local)?;
    let parent = branch.get().peel_to_commit()?;
    let sig = repo.signature()?;

    let msg = match msg {
      Some(msg) => msg,
      None => {
        // use parent commit message
        &format!("WIP: {}", match parent.summary()? {
          Some(it) => it,
          // TODO: truncate message
          None => parent.message()?,
        })
      }
    };

    // build tree of changes
    let tree = if staged {
      let mut index = repo.index()?;
      let tree_id = index.write_tree()?;
      repo.find_tree(tree_id)?
    } else {
      let base_tree = head.peel_to_tree()?;

      let mut opts = DiffOptions::new();
      opts.include_untracked(untracked);
      opts.recurse_untracked_dirs(untracked);
      let diff = repo.diff_tree_to_workdir(Some(&base_tree), Some(&mut opts))?;

      let mut index = repo
        .apply_to_tree(&base_tree, &diff, None)
        .context("Failed to build wip changes")?;

      let tree_id = index.write_tree_to(repo)?;
      repo.find_tree(tree_id)?
    };

    // TODO: add more parents to diff between staged, unstaged, and untracked files
    // in wip commit
    let parents: Vec<Commit> = vec![parent];

    // create wip commit
    let id = repo.commit(
      None,
      &sig,
      &sig,
      msg,
      &tree,
      // map from Vec<Commit> to Vec<&Commit>
      &parents.iter().collect::<Vec<_>>(),
    )?;

    // create/update ref/reflog
    match repo.find_reference(&self.refname).not_found_ok()? {
      // a wip ref exists for this branch, update it
      Some(mut rf) => {
        rf.set_target(id, msg)?;

        // re-read reflog from disk
        self.reflog = repo.reflog(&self.refname)?;
      }

      // no wip ref exists for this branch, create it
      None => {
        repo.reference(&self.refname, id, false, msg)?;
        self.reflog.append(id, &sig, Some(msg))?;
        self.reflog.write()?;
      }
    };

    let wip = Wip::from_reflog_entry(
      self
        .reflog
        .get(0)
        .context("Failed to get newly created reflog entry")?,
      self.branch.clone(),
      0,
    );

    // remove changes from workdir
    if !keep {
      if staged {
        // existing reference to head hasn't changed
        let base_tree = head.peel_to_tree()?;

        let mut opts = DiffOptions::new();
        opts.include_untracked(untracked);
        opts.recurse_untracked_dirs(untracked);
        let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;

        // index built from head + unstaged changes
        // `git stash --staged` can fail to apply here. it keeps the stash entry but
        // fails to remove changes from workdir
        let mut index = repo
          .apply_to_tree(&base_tree, &diff, None)
          .context("Wip was created, but changes cannot be removed from working directory")?;

        // checkout to update workdir
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.checkout_index(Some(&mut index), Some(&mut checkout))?;

        // update index to match head
        let mut index = repo.index()?;
        index.read_tree(&repo.head()?.peel_to_tree()?)?;
        index.write()?;
      } else {
        // just reset to head
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.reset(
          // existing reference to head hasn't changed
          head.peel_to_commit()?.as_object(),
          git2::ResetType::Hard,
          Some(&mut checkout),
        )?;
      }
    }

    Ok(wip)
  }
}

pub struct WipIter<'list> {
  range: Range<usize>,
  list: &'list WipList,
}

impl<'list> Iterator for WipIter<'list> {
  type Item = Wip<'list>;

  fn next(&mut self) -> Option<Self::Item> {
    self.range.next().and_then(|i| self.list.get(i))
  }
}
