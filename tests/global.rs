//! Tests global command args

use std::fs;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::TempDir;

use crate::common::TestRepo;

mod common;

/// Feature should use the manually specified git dir
#[test]
fn uses_specified_git_dir() {
  let repo = TestRepo::new();
  let file_name = "file.txt";
  repo.write_file(file_name, "A");
  repo.git(&["add", file_name]).success();

  // call feature from another dir
  let other_dir = TempDir::with_prefix("other-dir-").unwrap();
  cargo_bin_cmd!()
    .current_dir(other_dir.path())
    .args(["--git-dir", path_str!(repo.path()), "commit", "A"])
    .assert()
    .success();

  assert_eq!(repo.list_commit_subjects("main").trim(), "A");
}

#[test]
fn uses_specified_dir_and_worktree() {
  let repo = TestRepo::new_bare();
  let wt = TempDir::with_prefix("worktree-").unwrap();
  let somewhere = TempDir::with_prefix("other-dir-").unwrap();
  let file_name = "file.txt";

  let git = |args: &[&str]| {
    Command::new("git")
      .current_dir(somewhere.path())
      .args([
        "--git-dir",
        path_str!(repo.path()),
        "--work-tree",
        path_str!(wt.path()),
      ])
      .args(args)
      .assert()
  };

  let feature = |args: &[&str]| {
    cargo_bin_cmd!()
      .current_dir(somewhere.path())
      .args([
        "--git-dir",
        path_str!(repo.path()),
        "--worktree",
        path_str!(wt.path()),
      ])
      .args(args)
      .assert()
  };

  git(&["checkout", "-b", "main"]).success();

  fs::write(wt.path().join(file_name), "A").unwrap();
  git(&["add", file_name]).success();
  feature(&["commit", "A"]).success();

  let cmd = git(&["log", "--pretty=format:%s", "main"]).success();
  assert_eq!(get_stdout!(cmd).trim(), "A");
}

/// Specifying --worktree in the command line should error if no --git-dir is specified
#[test]
fn worktree_requires_git_dir() {
  let repo = TestRepo::new();
  let cmd = repo.feature(&["--worktree", "anywhere", "st"]).failure();
  assert!(
    get_stderr!(cmd).trim().starts_with(
      r"error: the following required arguments were not provided:
  --git-dir <GIT_DIR>"
    ),
    "stderr contains the wrong error message"
  );
}
