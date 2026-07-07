use std::fs;

use crate::common::TestRepo;

mod common;

#[test]
fn creates_stash_commit() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");

  repo.feature(&["stash", "push", "wip on main"]).success();

  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/stashes/main",
    ])
    .success()
    .stdout("wip on main\n");

  // first parent commit of stash should be A
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/stashes/main^1",
    ])
    .success()
    .stdout("A\n");

  // reflog exists for stash ref
  repo
    .git(&["reflog", "--format=%s", "refs/feature/stashes/main"])
    .success()
    .stdout("wip on main\n");

  // changes are removed from workdir
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "File contents should be reset by stash"
  );
}

/// Pushing a stash should stack on top of previous stashes
#[test]
fn pushes_stack() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["stash", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["stash", "push", "wip c"]).success();

  // reflog contains all stashes, with wip c on top
  repo
    .git(&["reflog", "--format=%s", "refs/feature/stashes/main"])
    .success()
    .stdout("wip c\nwip b\n");
}

/// Feature should be able to just stash unstaged changes
#[test]
fn stashes_unstaged_only() {
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
    .feature(&["stash", "push", "--staged", "wip on main"])
    .success();

  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/stashes/main",
    ])
    .success()
    .stdout("wip on main\n");

  // changes are removed from workdir
  assert_eq!(
    fs::read_to_string(repo.path().join("file.txt")).unwrap(),
    "one\ntwo\nthree\n",
    "file.txt contents should be reset by stash"
  );

  assert_eq!(
    fs::read_to_string(repo.path().join("file2.txt")).unwrap(),
    "one\ntwo\n",
    "file2.txt contents should not be reset by stash"
  );
}
