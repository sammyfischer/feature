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

  let dir_wt_args = [
    "--git-dir",
    path_str!(repo.path()),
    "--work-tree",
    path_str!(wt.path()),
  ];

  let git = |args: &[&str]| {
    Command::new("git")
      .current_dir(somewhere.path())
      .args(dir_wt_args)
      .args(args)
      .assert()
  };

  let feature = |args: &[&str]| {
    cargo_bin_cmd!()
      .current_dir(somewhere.path())
      .args(dir_wt_args)
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

/// Specifying --worktree in the command line should error if no --git-dir is
/// specified
#[test]
fn worktree_requires_git_dir() {
  let repo = TestRepo::new();
  let cmd = repo.feature(&["--work-tree", "anywhere", "st"]).failure();

  assert!(
    get_stderr!(cmd).trim().starts_with(
      r"error: the following required arguments were not provided:
  --git-dir <GIT_DIR>"
    ),
    "Should print the correct error message"
  );
}

fn setup_feature_project() -> (TestRepo, TestRepo) {
  let repo = TestRepo::new();
  let project_remote = TestRepo::new();
  project_remote.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--url",
      project_remote.path().to_str().unwrap(),
      "--path",
      // test searching up multiple dirs
      "packages/types",
      "types",
    ])
    .success();

  repo.feature(&["sync"]).success();

  (repo, project_remote)
}

/// If the current repo is a feature project, it should try searching for a
/// parent config
#[test]
fn project_layers_parent_config() {
  let (parent, _project) = setup_feature_project();
  let home = parent.path().parent().unwrap();

  parent.write_file(
    "feature.toml",
    r#"[branch]
sep = "-"
template = "%(user)/%s"
"#,
  );

  Command::new("git")
    .current_dir(parent.path().join("packages/types"))
    .env("HOME", home)
    .args(["config", "feature.user", "testuser"])
    .assert()
    .success();

  // package should use the parent project's template
  cargo_bin_cmd!()
    .current_dir(parent.path().join("packages/types"))
    .env("HOME", home)
    .args(["start", "new", "branch"])
    .assert()
    .success()
    .stdout("Created testuser/new-branch (from main)\n");

  // double check that the branch exists
  Command::new("git")
    .current_dir(parent.path().join("packages/types"))
    .env("HOME", home)
    .args(["branch"])
    .assert()
    .success()
    .stdout("  main\n* testuser/new-branch\n");
}

/// Feature should not search upward if a project contains its own config
#[test]
fn project_uses_own_config() {
  let (repo, _project) = setup_feature_project();
  let home = repo.path().parent().unwrap();

  repo.write_file(
    "feature.toml",
    r#"[branch]
sep = "-"
template = "%(user)/%s"
"#,
  );

  // different config in project
  fs::write(
    repo.path().join("packages/types/feature.toml"),
    r#"[branch]
sep = "-"
template = "%(base)-%s"
"#,
  )
  .unwrap();

  Command::new("git")
    .current_dir(repo.path().join("packages/types"))
    .env("HOME", home)
    .args(["config", "feature.user", "testuser"])
    .assert()
    .success();

  // package should use the parent project's template
  cargo_bin_cmd!()
    .current_dir(repo.path().join("packages/types"))
    .env("HOME", home)
    .args(["start", "new", "branch"])
    .assert()
    .success()
    .stdout("Created main-new-branch (from main)\n");

  // double check that the branch exists
  Command::new("git")
    .current_dir(repo.path().join("packages/types"))
    .env("HOME", home)
    .args(["branch"])
    .assert()
    .success()
    .stdout("  main\n* main-new-branch\n");
}
