use std::fs;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::TempDir;

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

/// Pushing should use the already-existing with a name that differs from the current branch name
#[test]
fn pushes_to_upstream_with_different_name() {
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
    local.list_commit_hashes("feature1"),
    remote.list_commit_hashes("feature1"),
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
    local.list_commit_hashes("feature1"),
    remote.list_commit_hashes("feature1"),
    "Commits should be different after new local changes"
  );

  local.feature(&["push"]).failure();
}

/// Base branches should be pushed if they're fast-forwardable
#[test]
fn pushes_base_branch() {
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.feature(&["push"]).success();

  assert_eq!(
    local.list_commit_hashes("main"),
    remote.list_commit_hashes("main"),
    "Local and remote should have the same commits on main"
  );
}

/// Pushes to a remote other than the default
#[test]
fn pushes_to_different_remote() {
  let local = TestRepo::new();
  let remote_a = TestRemote::new();
  let remote_b = TestRemote::new();

  local
    .git(&["remote", "add", "remote-a", path_str!(remote_a.path())])
    .success();
  local
    .git(&["remote", "add", "remote-b", path_str!(remote_b.path())])
    .success();

  local.init_commit();
  local.feature(&["start", "to", "remote", "a"]).success();
  local.write_file("a.txt", "a");
  local.commit_all("a");
  local.feature(&["push", "--remote", "remote-a"]).success();

  local.git(&["switch", "main"]).success();
  local.feature(&["start", "to", "remote", "b"]).success();
  local.write_file("b.txt", "b");
  local.commit_all("b");
  local.feature(&["push", "--remote", "remote-b"]).success();

  let cmd = local
    .git(&["branch", "--list", "--format=%(refname) %(upstream)"])
    .success();
  let stdout = get_stdout!(cmd);

  assert!(
    stdout.contains("refs/heads/to-remote-a refs/remotes/remote-a/to-remote-a"),
    "Branch a should be pushed to remote a"
  );

  assert!(
    stdout.contains("refs/heads/to-remote-b refs/remotes/remote-b/to-remote-b"),
    "Branch b should be pushed to remote b"
  );
}

/// If a branch has an existing upstream other than default, it should push to that with no args
#[test]
fn pushes_to_existing_different_remote() {
  let local = TestRepo::new();
  let remote_a = TestRemote::new();

  local
    .git(&["remote", "add", "remote-a", path_str!(remote_a.path())])
    .success();

  local.init_commit();
  local.feature(&["start", "to", "remote", "a"]).success();
  local
    .git(&["push", "-u", "remote-a", "to-remote-a"])
    .success();
  local.write_file("a.txt", "a");
  local.commit_all("a");
  local.feature(&["push"]).success();

  let cmd = local
    .git(&["branch", "--list", "--format=%(refname) %(upstream)"])
    .success();
  let stdout = get_stdout!(cmd);

  assert!(
    stdout.contains("refs/heads/to-remote-a refs/remotes/remote-a/to-remote-a"),
    "Branch a should be pushed to remote a"
  );
}

/// User should be able to choose upstream name with `--upstream`
#[test]
fn pushes_with_custom_upstream() {
  let local = TestRepo::new();
  let remote = TestRemote::new();
  local.init_commit();
  local
    .git(&["remote", "add", "the-origin", path_str!(remote.path())])
    .success();

  local.feature(&["start", "branch"]).success();
  local
    .feature(&["push", "-u", "custom-name", "-r", "the-origin"])
    .success();

  let cmd = local
    .git(&["branch", "--list", "--format=%(refname) %(upstream)"])
    .success();
  let stdout = get_stdout!(cmd);

  assert!(
    stdout.contains("refs/heads/branch refs/remotes/the-origin/custom-name"),
    "Upstream should use the custom name"
  );
}

/// Push should succeed in a bare repo
#[test]
fn pushes_in_bare_repo() {
  let repo = TestRepo::new_bare();
  let wt = TempDir::with_prefix("repo-worktree-").unwrap();
  let remote = TestRemote::new();
  let file_name = "file.txt";

  let git = |args: &[&str]| {
    Command::new("git")
      .current_dir(wt.path())
      .args([
        "--git-dir",
        path_str!(repo.path()),
        "--work-tree",
        path_str!(wt.path()),
      ])
      .args(args)
      .assert()
  };

  let feature = |args: &[&str]| {
    cargo_bin_cmd!()
      .current_dir(wt.path())
      .args([
        "--git-dir",
        path_str!(repo.path()),
        "--worktree",
        path_str!(wt.path()),
      ])
      .args(args)
      .assert()
  };

  git(&["remote", "add", "origin", path_str!(remote.path())]).success();

  fs::write(wt.path().join(file_name), "A").unwrap();
  git(&["add", file_name]).success();
  git(&["commit", "-m", "A"]).success();

  // first push, no upstream config
  feature(&["push"]).success();

  fs::write(wt.path().join(file_name), "B").unwrap();
  git(&["add", file_name]).success();
  git(&["commit", "-m", "B"]).success();

  // second push, upstream config exists
  feature(&["push"]).success();

  // make sure they got pushed
  assert_eq!(remote.list_commit_subjects("main").trim(), "B\nA");
}

/// Push should fail if upstream branch is diverged, even when the user hasn't fetched
#[test]
fn fails_when_upstream_diverges() {
  let (local, remote) = TestRepo::new_with_remote();
  let file_name = "file.txt";
  local.write_file(file_name, "A");
  local.commit_all("A");
  local.feature(&["push"]).success();

  // create the branch and push
  local.feature(&["start", "topic"]).success();
  local.feature(&["push"]).success(); // push before changes

  // push new changes from repo 2
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.git(&["switch", "topic"]).success();
  local2.write_file(file_name, "X");
  local2.commit_all("X");
  local2.feature(&["push"]).success();

  // create commit in repo 1 without fetching first
  local.write_file(file_name, "B");
  local.commit_all("B");

  // branches diverged
  let cmd = local.feature(&["push"]).failure();
  let stderr = get_stderr!(cmd);

  assert!(
    stderr.starts_with("Error: Branch has diverged from its upstream"),
    "Error should use the custom error message. Instead, the message was: {}",
    stderr
  );
}

/// Push should fail if base branch is diverged, even when the user hasn't fetched
#[test]
fn fails_when_base_diverges() {
  let (local, remote) = TestRepo::new_with_remote();
  let file_name = "file.txt";
  local.write_file(file_name, "A");
  local.commit_all("A");
  // push main to create its upstream
  local.feature(&["push"]).success();

  // create the branch and push
  local.feature(&["start", "topic"]).success();
  local.feature(&["push"]).success();

  // push new changes from repo 2
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file(file_name, "C");
  local2.commit_all("C");
  local2.feature(&["push"]);

  // create commit in repo 1 without fetching first
  local.write_file(file_name, "B");
  local.commit_all("B");

  // branches diverged
  let cmd = local.feature(&["push"]).failure();
  let stderr = get_stderr!(cmd);

  assert!(
    stderr.starts_with("Error: Branch has diverged from its base"),
    "Error should use the custom error message. Instead, the message was: {}",
    stderr
  );
}
