use crate::common::TestRepo;

mod common;

/// Should delete branches with upstreams that are redundant (behind or equal to their base)
#[test]
fn deletes_merged_branches() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.git(&["push", "-u", "origin", "main"]).success();

  // create branches. don't commit so that their commit history is identical to main
  for branch in ["feature1", "feature2"] {
    local.feature(&["start", branch]).success();
    local.git(&["switch", "main"]).success();
    local.git(&["push", "-u", "origin", branch]).success();
  }

  // prune should delete them since all their commits are in main
  local.feature(&["prune"]).success();

  // check that they no longer exist
  let cmd = local.git(&["branch"]).success();
  let text = get_stdout!(cmd);

  // branches and their config are deleted
  for branch in ["feature1", "feature2"] {
    assert!(!text.contains(branch));
    local
      .git(&["config", &format!("branch.{}.feature-base", branch)])
      .failure();
  }
}

#[test]
fn perserves_unmerged_branches() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.git(&["push", "-u", "origin", "main"]);

  // create a feature branch and commit
  local.feature(&["start", "feature"]).success();
  local.write_file("file.txt", "feature impl");
  local.commit_all("impl feature");
  local.git(&["switch", "main"]).success();

  // prune shouldn't delete feature 1
  local.feature(&["prune"]).success();

  // check that only correct branches were deleted
  let cmd = local.git(&["branch"]);
  let text = get_stdout!(cmd);

  // feature1 and its config should exist
  assert!(text.contains("feature"));
  local
    .git(&["config", "branch.feature.feature-base"])
    .success();
}

/// Should not delete branches that were never pushed
#[test]
fn preserves_unpushed_branches() {
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

  // branches and their config are deleted
  for branch in ["feature1", "feature2"] {
    assert!(text.contains(branch));
    local
      .git(&["config", &format!("branch.{}.feature-base", branch)])
      .success();
  }
}

/// Running with --dry-run should print candidates but not delete any branches or modify config
#[test]
fn dry_run_doesnt_delete() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.git(&["push", "-u", "origin", "main"]).success();

  // branches with identical history to main
  for branch in ["feature1", "feature2"] {
    local.feature(&["start", branch]).success();
    local.git(&["switch", "main"]).success();
  }

  local.feature(&["prune", "--dry-run"]).success();

  // check that they still exist
  let cmd = local.git(&["branch"]).success();
  let text = get_stdout!(cmd);

  // check that branches and their config entries exist
  for branch in ["feature1", "feature2"] {
    assert!(text.contains(branch));
    local
      .git(&["config", &format!("branch.{}.feature-base", branch)])
      .success();
  }
}
