use std::fs;

use crate::common::TestRepo;

mod common;

#[test]
fn deletes_merged_branches() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.git(&["push", "-u", "origin", "main"]).success();

  // create branches. don't commit so that their commit history is identical to main
  for branch in ["feature1", "feature2"] {
    local.feature(&["start", branch]).success();
    local.git(&["switch", "main"]).success();
  }

  // prune should delete them since all their commits are in main
  local.feature(&["prune"]).success();

  // check that they no longer exist
  let cmd = local.git(&["branch"]).success();
  let text = String::from_utf8(cmd.get_output().stdout.clone()).unwrap();
  assert!(!text.contains("feature1"));
  assert!(!text.contains("feature2"));
}

#[test]
fn perserves_unmerged_branches() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.git(&["push", "-u", "origin", "main"]);

  // create a feature branch and commit
  local.feature(&["start", "feature1"]).success();
  fs::write(local.dir.path().join("file.txt"), "feature1 implementation").unwrap();
  local.git(&["add", "."]).success();
  local.feature(&["commit", "impl", "feature1"]).success();
  local.git(&["switch", "main"]).success();

  // create branches. don't commit so that their commit history is identical to main
  for branch in ["feature2", "feature3"] {
    local.feature(&["start", branch]).success();
    local.git(&["switch", "main"]).success();
  }

  // prune shouldn't delete feature 1
  local.feature(&["prune"]).success();

  // check that they no longer exist
  let cmd = local.git(&["branch"]);
  let text = String::from_utf8(cmd.get_output().stdout.clone()).unwrap();
  assert!(text.contains("feature1"));
  assert!(!text.contains("feature2"));
  assert!(!text.contains("feature3"));
}
