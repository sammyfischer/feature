use crate::common::TestRepo;

mod common;

/// If the base branch has no upstream, feature should set it as the feature-base
#[test]
fn sets_base() {
  let repo = TestRepo::new();
  repo.init_commit();

  repo.git(&["switch", "-c", "topic"]).success();
  repo.feature(&["base", "main"]).success();
  let cmd = repo.git(&["config", "branch.topic.feature-base"]).success();
  assert_eq!(get_stdout!(cmd).trim(), "refs/heads/main");
}

/// If the base branch has an upstream, feature should set the upstream as the feature-base
#[test]
fn sets_base_using_upstream() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  local.git(&["switch", "-c", "topic"]).success();
  local.feature(&["base", "main"]).success();

  let cmd = local
    .git(&["config", "branch.topic.feature-base"])
    .success();
  assert_eq!(get_stdout!(cmd).trim(), "refs/remotes/origin/main");
}

/// Should set the base of non-current branch when specified
#[test]
fn sets_base_of_another_branch() {
  let repo = TestRepo::new();
  repo.init_commit();

  repo.git(&["switch", "-c", "topic1"]).success();
  repo.git(&["switch", "-c", "topic2"]).success();
  repo
    .feature(&["base", "main", "--branch", "topic1"])
    .success();
  let cmd = repo
    .git(&["config", "branch.topic1.feature-base"])
    .success();
  assert_eq!(get_stdout!(cmd).trim(), "refs/heads/main");
}
