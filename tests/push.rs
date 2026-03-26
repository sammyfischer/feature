use crate::common::{TestRemote, TestRepo};

mod common;

/// Creates a local and remote repo, pushes main to remote with an initial commit, and then creates
/// a feature branch locally and commits to it
fn create_upstream_and_feature_branch() -> (TestRepo, TestRemote) {
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // create new branch and commit
  local.feature(&["start", "feature1"]).success();
  local.write_file("feature1.txt", "feature 1");
  local.git(&["add", "."]).success();
  local.feature(&["commit", "impl", "feature1"]).success();

  (local, remote)
}

/// Pushing should use the already-existing upstream branch if available
#[test]
fn pushes_to_upstream() {
  let (local, _remote) = create_upstream_and_feature_branch();

  // push to remote with different name
  local
    .git(&["push", "-u", "origin", "feature1:feature1-remote"])
    .success();

  // commit new changes
  local.write_file("feature1.txt", "feature 1 bugfix");
  local.git(&["add", "."]).success();
  local
    .feature(&["commit", "feature", "1", "bugfix"])
    .success();

  // push to existing upstream branch
  local.feature(&["push"]).success();

  let text = local.list_branches_and_upstreams();

  println!("{}", text);
  assert!(text.contains("refs/heads/feature1 refs/remotes/origin/feature1-remote"));
}

/// Pushing for the first time should create a new remote-tracking branch with the same name as the
/// branch
#[test]
fn creates_upstream() {
  let (local, _remote) = create_upstream_and_feature_branch();

  // push new branch
  local.feature(&["push"]).success();

  let text = local.list_branches_and_upstreams();

  println!("{}", text);
  assert!(text.contains("refs/heads/feature1 refs/remotes/origin/feature1"));
}

/// Force pushing should always overwrite the remote branch
#[test]
fn force_always_pushes() {
  let (local, remote) = create_upstream_and_feature_branch();
  local.feature(&["push"]).success();

  // create a new repo, push new commits to feature1
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.git(&["switch", "feature1"]).success();
  local2.write_file("feature1.txt", "modified feature 1");
  local2.git(&["add", "."]).success();
  local2.feature(&["commit", "changes from repo 2"]).success();
  local2.feature(&["push"]).success();

  local.write_file("feature1.txt", "conflict");
  local.git(&["add", "."]).success();
  local.feature(&["commit", "conflicting commit"]).success();
  local.feature(&["push", "-f"]).success();
}

/// Pushing (without force) should fail if remote is non fast-forwardable
#[test]
fn fails_when_remote_changes() {
  let (local, remote) = create_upstream_and_feature_branch();
  local.feature(&["push"]).success();

  assert_eq!(
    local.list_commits_on_branch("feature1"),
    remote.list_commits_on_branch("feature1"),
    "Commits should be the same after initial push"
  );

  // create a new repo, push new commits to feature1
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.git(&["switch", "feature1"]).success();
  local2.write_file("feature1.txt", "modified feature 1");
  local2.git(&["add", "."]).success();
  local2.feature(&["commit", "changes from repo 2"]).success();
  local2.feature(&["push"]).success();

  local.write_file("feature1.txt", "conflict");
  local.git(&["add", "."]).success();
  local.feature(&["commit", "conflicting commit"]).success();

  assert_ne!(
    local.list_commits_on_branch("feature1"),
    remote.list_commits_on_branch("feature1"),
    "Commits should be different after new local changes"
  );

  local.feature(&["push"]).failure();
}

#[test]
fn pushes_base_branch() {
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.feature(&["push"]).success();

  assert_eq!(
    local.list_commits_on_branch("main"),
    remote.list_commits_on_branch("main"),
    "Local and remote should have the same commits on main"
  );
}

#[test]
fn refuses_to_force_push_base_branch() {
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.feature(&["push", "-f"]).failure();

  assert_ne!(
    local.list_commits_on_branch("main"),
    remote.list_commits_on_branch("main"),
    "Local and remote main should be different after push fails"
  );
}
