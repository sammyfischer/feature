use std::fs;

use crate::common::{init_commit, init_repo_with_remote, run_feature, run_git};

mod common;

#[test]
fn deletes_merged_branches() {
  let (local, _remote) = init_repo_with_remote();
  init_commit(&local);

  run_git(&["push", "-u", "origin", "main"], local.path());

  // create branches. don't commit so that their commit history is identical to main
  for branch in ["feature1", "feature2"] {
    run_feature(&["start", branch], local.path()).success();
    run_git(&["switch", "main"], local.path()).success();
  }

  // prune should delete them since all their commits are in main
  run_feature(&["prune"], local.path()).success();

  // check that they no longer exist
  let cmd = run_git(&["branch"], local.path());
  let text = String::from_utf8(cmd.get_output().stdout.clone()).unwrap();
  assert!(!text.contains("feature1"));
  assert!(!text.contains("feature2"));
}

#[test]
fn perserves_unmerged_branches() {
  let (local, _remote) = init_repo_with_remote();
  init_commit(&local);

  run_git(&["push", "-u", "origin", "main"], local.path());

  // create a feature branch and commit
  run_feature(&["start", "feature1"], local.path()).success();
  fs::write(local.path().join("file.txt"), "feature1 implementation").unwrap();
  run_git(&["add", "."], local.path()).success();
  run_feature(&["commit", "impl", "feature1"], local.path()).success();
  run_git(&["switch", "main"], local.path()).success();

  // create branches. don't commit so that their commit history is identical to main
  for branch in ["feature2", "feature3"] {
    run_feature(&["start", branch], local.path()).success();
    run_git(&["switch", "main"], local.path()).success();
  }

  // prune shouldn't delete feature 1
  run_feature(&["prune"], local.path()).success();

  // check that they no longer exist
  let cmd = run_git(&["branch"], local.path());
  let text = String::from_utf8(cmd.get_output().stdout.clone()).unwrap();
  assert!(text.contains("feature1"));
  assert!(!text.contains("feature2"));
  assert!(!text.contains("feature3"));
}
