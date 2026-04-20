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

/// Gets the subject line of a commit given its hash
fn hash_to_subject(repo: &TestRepo, hash: &str) -> String {
  // --no-patch removes the diff output, which git show includes by default
  let cmd = repo.git(&["show", "--no-patch", "--pretty=format:%s", hash]);
  get_stdout!(cmd)
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
  local.write_file("file.txt", "A");
  local.commit_all("A");
  local.feature(&["push"]).success();

  // brand new file
  local.feature(&["start", "feature"]).success();
  local.write_file("feature.txt", "X");
  local.commit_all("X");
  local.feature(&["push"]).success();

  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file("main.txt", "B");
  local2.commit_all("B");
  local2.feature(&["push"]).success();

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

/// If the rebase stops due to a conflict, the remaining steps should be written to git-rebase-todo.
/// The current file should also exist and tell libgit2 where to resume from.
#[test]
fn rebase_dumps_todo_file() {
  let repo = create_conflicts();

  repo.git(&["switch", "feature"]).success();
  repo.write_file("feature.txt", "Y");
  repo.commit_all("Y");
  repo.feature(&["update"]).failure();

  let current_hash = fs::read_to_string(repo.path().join(".git/rebase-merge/current"))
    .expect("current file should exist");

  assert_eq!(hash_to_subject(&repo, &current_hash), "X");

  let todo_file = fs::read_to_string(repo.path().join(".git/rebase-merge/git-rebase-todo"))
    .expect("git-rebase-todo file should exist");

  // the subject-line of all remaining commits in the rebase
  let mut remaining_commits: Vec<String> = Vec::new();
  for step in todo_file.lines() {
    let mut parts = step.split(" ");
    assert_eq!(
      parts.next(),
      Some("pick"),
      "Each command in todo file should be pick"
    );

    let hash = parts
      .next()
      .expect("Each line of todo file should contain a hash");

    remaining_commits.push(hash_to_subject(&repo, hash));
  }

  assert_eq!(remaining_commits.join(", "), "Y");
}

/// If the rebase stops due to a conflict, it should create a todo file even if there is nothing
/// left to do
#[test]
fn rebase_dumps_empty_todo_file() {
  let repo = create_conflicts();

  repo.git(&["switch", "feature"]).success();
  repo.feature(&["update"]).failure();

  let todo = fs::read_to_string(repo.path().join(".git/rebase-merge/git-rebase-todo"))
    .expect("git-rebase-todo file should exist");

  assert!(
    todo.is_empty(),
    "Todo file should only contian an empty string"
  );
}

/// An unfinished rebase should be compatible with `git rebase --continue`
#[test]
fn git_rebase_continues() {
  let repo = create_conflicts();

  repo.git(&["switch", "feature"]).success();
  repo.write_file("feature.txt", "Y");
  repo.commit_all("Y");
  repo.feature(&["update"]).failure();
  assert!(repo.is_rebase_active(), "Rebase should be active");

  // combine and resolve conflicting changes
  repo.write_file("file.txt", "BX");
  repo.git(&["add", "file.txt"]).success();
  repo.git(&["commit", "--amend", "-m", "BX"]).success();

  let todo = fs::read_to_string(repo.path().join(".git/rebase-merge/git-rebase-todo"))
    .expect("Todo file should exist");
  println!("{}", todo);

  repo.git(&["rebase", "--continue"]).success();
  assert!(!repo.is_rebase_active(), "Rebase should not be active");

  assert_eq!(repo.list_commit_subjects("feature"), "Y\nBX\nA")
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

/// Should be able to run `git rebase --skip` to skip current commit in a rebase started by feature
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

  // repo.feature(&["update", "-s"]).success();
  repo.git(&["rebase", "--skip"]).success();
  assert!(!repo.is_rebase_active(), "Rebase should not be active");

  assert_eq!(repo.list_commit_subjects("feature"), "Y\nC\nB\nA",);
}
