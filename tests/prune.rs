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
  let text = get_stdout!(cmd);
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
  local.write_file("file.txt", "feature1 impl");
  local.commit_all("impl feature1");
  local.git(&["switch", "main"]).success();

  // create branches. don't commit so that their commit history is identical to main
  for branch in ["feature2", "feature3"] {
    local.feature(&["start", branch]).success();
    local.git(&["switch", "main"]).success();
  }

  // prune shouldn't delete feature 1
  local.feature(&["prune"]).success();

  // check that only correct branches were deleted
  let cmd = local.git(&["branch"]);
  let text = get_stdout!(cmd);
  assert!(text.contains("feature1"));
  assert!(!text.contains("feature2"));
  assert!(!text.contains("feature3"));
}

/// Running with --dry-run should print candidates but not delete any branches or modify config
#[test]
fn dry_run_prints_candidates() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.git(&["push", "-u", "origin", "main"]).success();

  // branches with identical history to main
  for branch in ["feature1", "feature2"] {
    local.feature(&["start", branch]).success();
    local.git(&["switch", "main"]).success();
  }

  let cmd = local.feature(&["prune", "--dry-run"]).success();
  let stdout = get_stdout!(cmd);

  assert_eq!(stdout.trim(), "Deletion candidates:\nfeature1\nfeature2");

  // check that they still exist
  let cmd = local.git(&["branch"]).success();
  let text = get_stdout!(cmd);
  assert!(text.contains("feature1"));
  assert!(text.contains("feature2"));
}
