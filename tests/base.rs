use crate::common::TestRepo;

mod common;

/// If the base branch has no upstream, feature should set it as the feature-base
#[test]
fn sets_feature_base() {
  let repo = TestRepo::new();
  repo.init_commit();

  repo.git(&["switch", "-c", "topic"]).success();
  repo.feature(&["base", "main"]).success();
  let proc = repo.git(&["config", "branch.topic.feature-base"]).success();
  assert_eq!(get_stdout!(proc).trim(), "refs/heads/main");
}

/// If the base branch has an upstream, feature should set the upstream as the feature-base
#[test]
fn sets_feature_base_using_remote() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  local.git(&["switch", "-c", "topic"]).success();
  local.feature(&["base", "main"]).success();
  let proc = local
    .git(&["config", "branch.topic.feature-base"])
    .success();
  assert_eq!(get_stdout!(proc).trim(), "refs/remotes/origin/main");
}
