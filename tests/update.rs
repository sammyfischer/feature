use std::fs;

use crate::common::TestRepo;

mod common;

/// Creates a test repo with a main and feature branch that currently have conflicts. Leaves
/// repository checked out to main.
///
/// Creates branches with this structure:
/// ```txt
/// A - B <- main
///  \
///   X <- feature
/// ```
///
/// Where commits B and X modify `file.txt`, entirely replacing the contents with "B"
/// and "X" respectively.
fn create_conflicts() -> TestRepo {
  let repo = TestRepo::new();
  let file_name = "file.txt";
  repo.write_file(file_name, "A");
  repo.commit_all("A");

  repo.feature(&["start", "feature"]).success();
  repo.write_file(file_name, "X");
  repo.commit_all("X");

  repo.git(&["switch", "main"]).success();
  repo.write_file(file_name, "B");
  repo.commit_all("B");

  repo
}

/// Updating should rebase changes from main when there are no conflicts
#[test]
fn rebases_changes() {
  let repo = TestRepo::new();
  repo.write_file("file.txt", "A");
  repo.commit_all("A");

  // brand new file
  repo.feature(&["start", "feature"]).success();
  repo.write_file("feature.txt", "X");
  repo.commit_all("X");

  repo.git(&["switch", "main"]).success();
  repo.write_file("main.txt", "B");
  repo.commit_all("B");

  repo.git(&["switch", "feature"]).success();
  repo.feature(&["update"]).success();

  assert_eq!(
    repo.list_commit_subjects("feature"),
    "X\nB\nA",
    "feature should be rebased onto main"
  );
}

/// If base is a remote, it should be automatically fetched before updating
#[test]
fn auto_fetches_base() {
  let (local, remote) = TestRepo::new_with_remote();
  // commit A to main and push
  local.write_file("file.txt", "A");
  local.commit_all("A");
  local.git(&["push", "-u", "origin", "main"]).success();

  // commit X to feature (in new file) and push
  local.feature(&["start", "feature"]).success();
  local.write_file("feature.txt", "X");
  local.commit_all("X");
  local.git(&["push", "-u", "origin", "feature"]).success();

  // commit B to main and push
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file("main.txt", "B");
  local2.commit_all("B");
  local2.git(&["push", "-u", "origin", "main"]).success();

  local.feature(&["update"]).success();

  assert_eq!(
    local.list_commit_subjects("feature"),
    "X\nB\nA",
    "feature should be rebased onto main"
  );
}

/// Feature should exit, pausing the rebase, when there are conflicts
#[test]
fn rebase_stops_when_conflicts() {
  let repo = create_conflicts();

  repo.git(&["switch", "feature"]).success();

  // there will be merge conflicts, but it should exit successfully
  repo.feature(&["update"]).failure();
  assert!(repo.is_rebase_active(), "Rebase should be active");
}

/// Feature should continue an existing rebase when running with -c
#[test]
fn rebase_continues() {
  let repo = create_conflicts();

  repo.git(&["switch", "feature"]).success();
  repo.feature(&["update"]).failure();
  assert!(repo.is_rebase_active(), "Rebase should be active");

  // combine and resolve conflicting changes
  repo.write_file("file.txt", "BX");
  repo.git(&["add", "file.txt"]).success();
  repo.git(&["commit", "--amend", "-m", "BX"]).success();

  repo.feature(&["update", "-c"]).success();
  assert!(!repo.is_rebase_active(), "Rebase should not be active");

  assert_eq!(repo.list_commit_subjects("feature"), "BX\nA")
}

/// Feature should abort the rebase when running with -a
#[test]
fn rebase_aborts() {
  let repo = create_conflicts();

  repo.git(&["switch", "feature"]).success();
  repo.feature(&["update"]).failure();

  // don't resolve conflicts, just abort
  repo.feature(&["update", "-a"]).success();
  assert!(!repo.is_rebase_active(), "Rebase should not be active");

  assert_eq!(
    repo.list_commit_subjects("feature"),
    "X\nA",
    "feature's history should not have changed"
  );
  assert_eq!(
    repo.list_commit_subjects("main"),
    "B\nA",
    "main's history should not have changed"
  );
}

/// Feature should skip the current patch when running with -s
#[test]
fn git_rebase_skips() {
  let repo = create_conflicts();

  // still on main, create an unrelated commit
  repo.write_file("main.txt", "C");
  repo.commit_all("C");

  // also create an unrelated commit on feature
  repo.git(&["switch", "feature"]).success();
  repo.write_file("feature.txt", "Y");
  repo.commit_all("Y");

  // The branches currently look like:
  //
  // A - B - C <- main
  //  \
  //   X - Y <- feature
  //
  // Goal: skip commit X on feature, which conflicts with B on main, resulting in:
  //
  // A - B - C <- main
  //          \
  //           Y' <- feature
  //
  // Where Y' is an arbitrary name for the commit. The commit message of Y' is still "Y"

  repo.feature(&["update"]).failure();
  assert!(repo.is_rebase_active(), "Rebase should be active");

  println!(
    "Git todo file:\n{}",
    fs::read_to_string(repo.path().join(".git/rebase-merge/git-rebase-todo")).unwrap()
  );

  repo.feature(&["update", "-s"]).success();
  assert!(!repo.is_rebase_active(), "Rebase should not be active");

  assert_eq!(repo.list_commit_subjects("feature"), "Y\nC\nB\nA",);
}
