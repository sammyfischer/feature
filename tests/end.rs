use crate::common::TestRepo;

mod common;

/// End should delete the current branch if it's a feature branch
#[test]
fn deletes_current_branch() {
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  repo.feature(&["end"]).success();

  let cmd = repo.git(&["branch", "--show-current"]).success();
  assert_eq!(
    get_stdout!(cmd).trim(),
    "main",
    "main should be checked-out"
  );

  assert!(
    !repo
      .list_branches_and_upstreams()
      .contains("refs/heads/topic"),
    "topic should have been deleted"
  );
}

/// End should delete the specified non-current branch, and remain checked-out to the current branch.
#[test]
fn deletes_non_current_branch() {
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  repo
    .feature(&["start", "--from", "main", "other-topic"])
    .success();
  repo.feature(&["end", "topic"]).success();

  let cmd = repo.git(&["branch", "--show-current"]).success();
  assert_eq!(
    get_stdout!(cmd).trim(),
    "other-topic",
    "other-topic should be checked-out"
  );

  assert!(
    !repo
      .list_branches_and_upstreams()
      .contains("refs/heads/topic"),
    "topic should have been deleted"
  );
}

/// End should use the base specified by the user
#[test]
fn uses_specified_base() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "B");
  repo.commit_all("B");

  repo.feature(&["start", "other-topic"]).success();

  // this would succeed for topic, since they point to the same commit. but other-topic is ahead of main
  let cmd = repo.feature(&["end", "--base", "main"]).failure();
  assert_eq!(
    get_stderr!(cmd).trim(),
    "Error: other-topic is not merged into main",
    "Should print the correct error message"
  );
}

/// End should fail when the feature branch isn't merged into its base
#[test]
fn fails_when_unmerged() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "B");
  repo.commit_all("B");

  let cmd = repo.feature(&["end"]).failure();
  assert_eq!(
    get_stderr!(cmd).trim(),
    "Error: topic is not merged into main",
    "Should print the correct error message"
  );
}

/// End should succeed when branch is unmerged into base but --force is used
#[test]
fn succeeds_when_unmerged_force() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "B");
  repo.commit_all("B");

  repo.feature(&["end", "-f"]).success();

  let cmd = repo.git(&["branch", "--show-current"]).success();
  assert_eq!(
    get_stdout!(cmd).trim(),
    "main",
    "main should be checked-out"
  );

  assert!(
    !repo
      .list_branches_and_upstreams()
      .contains("refs/heads/topic"),
    "topic should have been deleted"
  );
}

/// End should fetch the latest base if base is a remote branch
#[test]
fn fetches_latest_base() {
  let file_name = "file.txt";
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // start feature branch
  local.feature(&["start", "topic"]).success();
  local.write_file(file_name, "B");
  local.commit_all("B");

  // commit new changes to main
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file("other-file.txt", "X");
  local2.commit_all("X");
  local2.git(&["push", "origin", "main"]).success();

  // end should fetch latest main
  let cmd = local.feature(&["end"]).success();
  assert!(
    get_stdout!(cmd).starts_with("Fetched origin/main"),
    "end should fetch latest origin/main"
  );
}

/// End should delete the branch on remote if --remote is specified
#[test]
fn deletes_from_remote() {
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  local.feature(&["start", "topic"]).success();
  local.git(&["push", "-u", "origin", "topic"]).success();

  local.feature(&["end", "-r"]).success();

  assert_eq!(
    remote.list_branches().trim(),
    "refs/heads/main",
    "topic should be deleted from remote"
  );

  assert!(
    !local
      .list_branches_and_upstreams()
      .contains("refs/remotes/origin/topic"),
    "origin/topic should be deleted from local"
  )
}

/// End should delete the branch's config in git config
#[test]
fn deletes_branch_config() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  local.feature(&["start", "topic"]).success();
  local.git(&["push", "-u", "origin", "topic"]).success();

  local.feature(&["end"]).success();

  local.git(&["config", "branch.topic.remote"]).code(1);
  local.git(&["config", "branch.topic.merge"]).code(1);
}
