use std::fs;

use assert_cmd::Command;

use crate::common::TestRepo;

mod common;

// Feature should remove metadata in feature.toml and gitignore
#[test]
fn removes_metadata() {
  // setup subproject
  let repo = TestRepo::new();
  let module = TestRepo::new();
  module.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--repo",
      module.path().to_str().unwrap(),
      "frontend",
    ])
    .success();

  repo.feature(&["project", "rm", "frontend"]).success();

  let config = fs::read_to_string(repo.path().join("feature.toml")).unwrap();
  assert_eq!(config, "[projects]\n");

  let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
  assert_eq!(ignore, "\n");

  // the repo itself should still exist
  Command::new("git")
    .current_dir(repo.path().join("frontend"))
    .env("HOME", repo.path().parent().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("A");
}
