use crate::common::TestRepo;

mod common;

#[test]
fn start_creates_branch() {
  for (args, expected) in [
    (vec!["start", "test"], "test"),
    (vec!["start", "new", "branch"], "new-branch"),
    (vec!["start", "feature/dark", "mode"], "feature/dark-mode"),
  ] {
    // new repo and tempdir for each test
    let repo = TestRepo::new();
    repo.init_commit();

    // run start command
    repo.feature(&args).success();

    // check current branch name
    let proc = repo.git(&["branch", "--show-current"]).success();
    let stdout = String::from_utf8(proc.get_output().stdout.clone()).unwrap();
    assert_eq!(stdout.trim(), expected.to_string());
  }
}

#[test]
fn empty_branch_name_fails() {
  let repo = TestRepo::new();
  repo.init_commit();
  // empty string
  repo.feature(&["start", ""]).failure();
}

#[test]
fn uses_custom_separator() {
  let repo = TestRepo::new();
  repo.init_commit();

  // using `--flag=value` syntax
  repo
    .feature(&["start", "--sep=_", "new", "branch"])
    .success();

  let proc = repo.git(&["branch", "--show-current"]).success();
  let Ok(stdout) = String::from_utf8(proc.get_output().stdout.clone()) else {
    panic!("Failed to get stdout as string")
  };

  assert_eq!(stdout.trim(), "new_branch".to_string());
}

#[test]
fn uses_custom_separator_alt_syntax() {
  let repo = TestRepo::new();
  repo.init_commit();

  // using `--flag value` syntax
  repo
    .feature(&["start", "--sep", "_", "new", "branch"])
    .success();

  let proc = repo.git(&["branch", "--show-current"]).success();
  let stdout = String::from_utf8(proc.get_output().stdout.clone()).unwrap();

  assert_eq!(stdout.trim(), "new_branch".to_string());
}

#[test]
fn only_starts_on_base_branch() {
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "feature1"]).success();
  repo.feature(&["start", "feature2"]).failure();
}
