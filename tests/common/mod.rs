use std::fs::write;
use std::path::Path;

use assert_cmd::assert::Assert;
use assert_cmd::{Command, cargo};
use tempfile::TempDir;

/// Creates a temp dir and initializes a git repo and commit signature
pub fn init_repo() -> TempDir {
  let dir = TempDir::new().unwrap();
  run_git(&["init", dir.path().to_str().unwrap()], dir.path()).success();

  run_git(&["config", "user.name", "test"], dir.path()).success();
  run_git(&["config", "user.email", "test@test.net"], dir.path()).success();

  dir
}

pub fn init_repo_with_remote() -> (TempDir, TempDir) {
  let local = init_repo();
  let remote = init_repo();

  run_git(
    &["remote", "add", "origin", remote.path().to_str().unwrap()],
    local.path(),
  )
  .success();

  (local, remote)
}

/// Creates a file, stages it, then commits to HEAD
pub fn init_commit(dir: &TempDir) {
  let file_name = "file.txt";

  write(dir.path().join(&file_name), "hello world").unwrap();

  run_git(&["add", &file_name], dir.path()).success();
  run_git(&["commit", "-m", "initial commit"], dir.path()).success();
}

pub fn run_feature(args: &[&str], cwd: &Path) -> Assert {
  cargo::cargo_bin_cmd!().current_dir(cwd).args(args).assert()
}

pub fn run_git(args: &[&str], cwd: &Path) -> Assert {
  Command::new("git").current_dir(cwd).args(args).assert()
}
