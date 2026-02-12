use crate::fixtures::*;

#[test]
fn start_creates_branch() {
  for (args, expected) in [
    // single-word branch name
    (vec!["start", "test"], "test"),
    // multi-word branch name
    (vec!["start", "new", "branch"], "new-branch"),
    (vec!["start", "feature/dark", "mode"], "feature/dark-mode"),
  ] {
    // new repo and tempdir for each test
    let (dir, repo) = init_repo();
    init_commit(&dir, &repo);

    // assert that command succeeds
    run(&args, dir.path()).assert().success();

    // assert that branch name matches
    let head = repo.head().unwrap();
    assert_eq!(head.shorthand(), Some(expected));
  }
}

#[test]
fn invalid_branch_name_fails() {
  let (dir, repo) = init_repo();
  init_commit(&dir, &repo);

  run(&["start", "$"], dir.path()).assert().failure();

  run(&["start", "new", "branch$"], dir.path())
    .assert()
    .failure();

  run(&["start", "br@nch"], dir.path()).assert().failure();
}
