use std::fs;

use crate::common::TestRepo;

mod common;

#[test]
fn creates_wip_commit() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");

  repo.feature(&["wip", "push", "wip on main"]).success();

  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main",
    ])
    .success()
    .stdout("wip on main\n");

  // first parent commit of wip should be A
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main^1",
    ])
    .success()
    .stdout("A\n");

  // reflog exists for wip ref
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip on main\n");

  // changes are removed from workdir
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "File contents should be reset by push"
  );
}

/// Pushing a wip should stack on top of previous wips
#[test]
fn pushes_to_top() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  // reflog contains all wips, with wip c on top
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip c\nwip b\n");
}

/// Feature should be able to just wip staged changes
#[test]
fn pushes_staged_only() {
  let repo = TestRepo::new();
  repo.write_file("file.txt", "a\n");
  repo.write_file("file2.txt", "a\n");
  repo.commit_all("initial commit");

  // staged change
  repo.write_file("file.txt", "b\n");
  repo.git(&["add", "file.txt"]).success();

  // unstaged change
  repo.write_file("file2.txt", "b\n");

  repo
    .feature(&["wip", "push", "--staged", "wip on main"])
    .success();

  assert_eq!(
    fs::read_to_string(repo.path().join("file.txt")).unwrap(),
    "a\n",
    "Staged changes should be reset by push"
  );

  assert_eq!(
    fs::read_to_string(repo.path().join("file2.txt")).unwrap(),
    "b\n",
    "Unstaged changes should not be reset by push"
  );
}

/// Feature should push untracked files when "-u" is specified
#[test]
fn pushes_untracked() {
  let repo = TestRepo::new();
  repo.init_commit();

  // edit tracked file, create untracked file
  repo.write_file("file.txt", "B\n");
  repo.write_file("file2.txt", "C\n");

  repo
    .feature(&["wip", "push", "-u", "wip on main"])
    .success();

  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main",
    ])
    .success()
    .stdout("wip on main\n");

  // first parent commit of wip should be A
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main^1",
    ])
    .success()
    .stdout("A\n");

  // reflog exists for wip ref
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip on main\n");

  // changes are removed from workdir
  assert_eq!(
    fs::read_to_string(repo.path().join("file.txt")).unwrap(),
    "A",
    "File contents should be reset by push"
  );

  assert!(
    !repo.path().join("file2.txt").exists(),
    "file2 should be removed from workdir"
  );
}

/// Feature should be able to push a wip to another branch
#[test]
fn push_to_different_branch() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();

  repo.write_file(file_name, "B\n");
  repo
    .feature(&["wip", "push", "-b", "main", "wip b"])
    .success();

  // reflog contains all wips, with wip c on top
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip b\n");
}
