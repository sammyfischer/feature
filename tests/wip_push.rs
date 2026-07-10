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
fn pushes_stack() {
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
  repo.write_file("file.txt", "one\ntwo\nthree\n");
  repo.write_file("file2.txt", "one\n");
  repo.commit_all("initial commit");

  // first commit:
  // > file.txt
  // one
  // two
  // three
  //
  // > file2.txt
  // one

  repo.write_file("file.txt", "one\n\nthree\n");
  repo.git(&["add", "file.txt"]).success();
  repo.write_file("file2.txt", "one\ntwo\n");

  // index:
  // > file.txt
  // one
  // three
  //
  // > file2.txt
  // one

  // workdir:
  // > file.txt
  // one
  // three
  //
  // > file2.txt
  // one
  // two

  repo
    .feature(&["wip", "push", "--staged", "wip on main"])
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

  // changes are removed from workdir
  assert_eq!(
    fs::read_to_string(repo.path().join("file.txt")).unwrap(),
    "one\ntwo\nthree\n",
    "file.txt contents should be reset by push"
  );

  assert_eq!(
    fs::read_to_string(repo.path().join("file2.txt")).unwrap(),
    "one\ntwo\n",
    "file2.txt contents should not be reset by push"
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
