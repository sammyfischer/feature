use std::fs::write;
use std::path::Path;

use assert_cmd::Command;
use git2::Repository;
use tempfile::{Builder, TempDir};

/// Creates a temp dir and initializes a git repo and commit signature
pub fn init_repo() -> (TempDir, Repository) {
  let dir = Builder::new()
    .tempdir_in(std::env::current_dir().unwrap())
    .unwrap();
  let repo = Repository::init(dir.path()).unwrap();

  let mut config = repo.config().unwrap();
  config.set_str("user.name", "test").unwrap();
  config.set_str("user.email", "test@test.net").unwrap();

  (dir, repo)
}

/// Creates a file, stages it, then commits to HEAD
pub fn init_commit(dir: &TempDir, repo: &Repository) {
  let file_name = "file.txt";

  write(dir.path().join(&file_name), "hello world").unwrap();

  let mut index = repo.index().unwrap();
  index.add_path(Path::new(&file_name)).unwrap();
  index.write().unwrap();

  let tree_id = index.write_tree().unwrap();
  let tree = repo.find_tree(tree_id).unwrap();
  let sig = repo.signature().unwrap();

  repo
    .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
    .unwrap();
}

pub fn run(args: &[&str], cwd: &Path) -> Command {
  let mut cmd = Command::cargo_bin("feature").unwrap();
  cmd.current_dir(cwd).args(args);
  cmd
}
