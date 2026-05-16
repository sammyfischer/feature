use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;

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

/// Prunes branches that were merged and deleted from remote
#[test]
fn prunes_merged_deleted_branches() {
  let local = TestRepo::new();

  // let remote be a non-bare repo, since merging in a bare repo is hard
  let remote = TestRepo::new();
  // check out to a different branch so pushes succeed
  remote.git(&["switch", "-c", "hidden-branch"]).success();

  local
    .git(&["remote", "add", "origin", remote.path().to_str().unwrap()])
    .success();

  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  local.feature(&["start", "topic"]).success();
  local.write_file("b.txt", "B");
  local.commit_all("B");
  local.git(&["push", "-u", "origin", "topic"]).success();

  // merge from remote, delete topic
  remote.git(&["switch", "main"]).success();
  remote
    .git(&["merge", "topic", "-m", "Merge branch 'topic' into main"])
    .success();
  remote.git(&["branch", "-d", "topic"]).success();

  local.git(&["switch", "main"]).success();
  local.feature(&["prune"]).success();

  let cmd = local.git(&["branch"]).success();
  let stdout = get_stdout!(cmd);
  println!("{}", stdout);

  assert!(!stdout.contains("topic"), "topic should be deleted");
}

/// Should prune branches that have been squashed and merged from remote
#[test]
fn prunes_with_squash_workflow() {
  let file_name = "file.txt";
  let local = TestRepo::new();
  let remote = TestRepo::new();
  local
    .git(&["remote", "add", "origin", remote.path().to_str().unwrap()])
    .success();
  remote.git(&["switch", "-c", "hidden-branch"]).success();

  // commit A to main
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // commit B1 and B2 to feature
  local.feature(&["start", "topic"]).success();
  for msg in ["B1", "B2"] {
    local.write_file(file_name, msg);
    local.commit_all(msg);
  }
  local.git(&["push", "-u", "origin", "topic"]).success();

  // squash and merge from remote like github
  remote.git(&["switch", "main"]).success();
  remote.git(&["merge", "--squash", "topic"]).success();
  remote.git(&["commit", "-m", "B"]).success();
  remote.git(&["branch", "-D", "topic"]).success();

  // update main branches locally
  local.git(&["switch", "main"]).success();
  local.git(&["pull"]).success();

  // ensure we have the squash commit
  assert_eq!(
    local.list_commit_subjects("main").trim(),
    "B\nA",
    "main should have the squash commit"
  );

  // ensure prune deletes topic
  local.feature(&["prune"]).success();
  assert!(
    !local
      .list_branches_and_upstreams()
      .trim()
      .contains("refs/heads/topic"),
    "topic should have been pruned"
  );
}

/// Prune should automatically fetch all branches
#[test]
fn autofetches() {
  let file_name = "file.txt";
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // local is ahead of origin/main in repo 1
  local.feature(&["start", "topic"]).success();
  local.write_file(file_name, "B");
  local.commit_all("B");
  local.git(&["push", "-u", "origin", "topic"]).success();

  // update main to topic
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.git(&["reset", "--hard", "origin/topic"]).success();
  local2.git(&["push", "-f", "origin", "main"]).success();

  // should autofetch new changes and realize it's merged
  local.git(&["switch", "main"]).success();
  local.feature(&["prune"]).success();
  assert!(
    !local
      .list_branches_and_upstreams()
      .contains("refs/heads/topic"),
    "topic should have been pruned"
  );
}

/// Prune should skip automatic fetch when `--no-fetch` is specified
#[test]
fn skips_autofetch_cli() {
  let file_name = "file.txt";
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // local is ahead of origin/main in repo 1
  local.feature(&["start", "topic"]).success();
  local.write_file(file_name, "B");
  local.commit_all("B");
  local.git(&["push", "-u", "origin", "topic"]).success();

  // update main to topic
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.git(&["reset", "--hard", "origin/topic"]).success();
  local2.git(&["push", "-f", "origin", "main"]).success();

  // shouldn't autofetch new changes, still thinks it's unmerged
  local.git(&["switch", "main"]).success();
  local.feature(&["prune", "--no-fetch"]).success();
  assert!(
    local
      .list_branches_and_upstreams()
      .contains("refs/heads/topic"),
    "topic should not have been pruned"
  );
}

/// Prune should skip automatic fetch when specified in git config
#[test]
fn skips_autofetch_config() {
  let file_name = "file.txt";
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // local is ahead of origin/main in repo 1
  local.feature(&["start", "topic"]).success();
  local.write_file(file_name, "B");
  local.commit_all("B");
  local.git(&["push", "-u", "origin", "topic"]).success();

  // update main to topic
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.git(&["reset", "--hard", "origin/topic"]).success();
  local2.git(&["push", "-f", "origin", "main"]).success();

  // shouldn't autofetch new changes, still thinks it's unmerged
  local.git(&["switch", "main"]).success();
  local.git(&["config", "feature.autofetch", "no"]).success();
  local.feature(&["prune"]).success();
  assert!(
    local
      .list_branches_and_upstreams()
      .contains("refs/heads/topic"),
    "topic should not have been pruned"
  );
}

/// Feature should prune branches in subprojects
#[test]
fn prunes_projects() {
  let repo = TestRepo::new();
  let home = repo.path().parent().unwrap();

  let frontend = TestRepo::new();
  let backend = TestRepo::new();
  frontend.init_commit();
  backend.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--repo",
      frontend.path().to_str().unwrap(),
      "frontend",
    ])
    .success();

  repo
    .feature(&[
      "project",
      "add",
      "--repo",
      backend.path().to_str().unwrap(),
      "backend",
    ])
    .success();

  for project in ["frontend", "backend"] {
    let path = repo.path().join(project);

    // new feature branch
    cargo_bin_cmd!()
      .current_dir(&path)
      .env("HOME", home)
      .args(["start", "topic"])
      .assert()
      .success();

    // push once so the branch is prunable
    Command::new("git")
      .current_dir(&path)
      .env("HOME", home)
      .args(["push", "-u", "origin", "topic"])
      .assert()
      .success();

    // switch off so the branch is prunable
    Command::new("git")
      .current_dir(&path)
      .env("HOME", home)
      .args(["switch", "main"])
      .assert()
      .success();
  }

  repo.feature(&["prune"]).success();

  for project in ["frontend", "backend"] {
    let path = repo.path().join(project);

    // topic branches should be deleted
    Command::new("git")
      .current_dir(&path)
      .env("HOME", home)
      .args(["branch", "--format=%(refname) %(upstream)"])
      .assert()
      .stdout("refs/heads/main refs/remotes/origin/main\n");
  }
}
