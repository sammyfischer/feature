use crate::fixtures::*;

#[test]
fn start_creates_branch() {
  for (args, expected) in [
    (vec!["start", "test"], "test"),
    (vec!["start", "new", "branch"], "new-branch"),
    (vec!["start", "feature/dark", "mode"], "feature/dark-mode"),
  ] {
    // new repo and tempdir for each test
    let dir = init_repo();
    init_commit(&dir);

    // run start command
    run_feature(&args, dir.path()).success();

    // check current branch name
    let proc = run_git(&["branch", "--show-current"], dir.path()).success();
    let Ok(stdout) = String::from_utf8(proc.get_output().stdout.clone()) else {
      panic!("Failed to get stdout as string")
    };
    assert_eq!(stdout.trim(), expected.to_string());
  }
}

#[test]
fn empty_branch_name_fails() {
  let dir = init_repo();
  init_commit(&dir);
  // empty string
  run_feature(&["start", ""], dir.path()).failure();
}

#[test]
fn uses_custom_separator() {
  let dir = init_repo();
  init_commit(&dir);

  // using `--flag=value` syntax
  run_feature(&["start", "--sep=_", "new", "branch"], dir.path()).success();

  let proc = run_git(&["branch", "--show-current"], dir.path()).success();
  let Ok(stdout) = String::from_utf8(proc.get_output().stdout.clone()) else {
    panic!("Failed to get stdout as string")
  };

  assert_eq!(stdout.trim(), "new_branch".to_string());
}

#[test]
fn uses_custom_separator_alt_syntax() {
  let dir = init_repo();
  init_commit(&dir);

  // using `--flag value` syntax
  run_feature(&["start", "--sep", "_", "new", "branch"], dir.path()).success();

  let proc = run_git(&["branch", "--show-current"], dir.path()).success();
  let Ok(stdout) = String::from_utf8(proc.get_output().stdout.clone()) else {
    panic!("Failed to get stdout as string")
  };

  assert_eq!(stdout.trim(), "new_branch".to_string());
}
