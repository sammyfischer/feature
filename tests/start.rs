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
    assert_eq!(repo.get_current_branch(), expected.to_string());
  }
}

#[test]
fn empty_branch_name_fails() {
  let repo = TestRepo::new();
  repo.init_commit();
  // empty string
  repo.feature(&["start", ""]).failure();
}

/// If the user specifies `--from` as a valid base, then start should start from that branch
#[test]
fn starts_from_specified_base() {
  let repo = TestRepo::new();
  repo.write_file("a.txt", "A");
  repo.commit_all("A");

  repo.git(&["switch", "-c", "dev"]).success();
  repo.write_file("b.txt", "B");
  repo.commit_all("B");

  repo.write_file("feature.toml", r#"bases = ["main", "dev"]"#);

  repo
    .feature(&["start", "--from", "main", "main-feature"])
    .success();

  // check that main-feature started from main
  assert_eq!(
    repo.list_commit_subjects("main"),
    repo.list_commit_subjects("main-feature"),
    "main and main-feature should have the same commit history"
  );

  // double check that we are checked out to the right branch
  assert_eq!(repo.get_current_branch(), "main-feature");
}

/// If the base branch has no upstream, feature should set it as the feature-base
#[test]
fn sets_feature_base() {
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  let cmd = repo.git(&["config", "branch.topic.feature-base"]).success();
  assert_eq!(get_stdout!(cmd).trim(), "refs/heads/main");
}

/// If the base branch has an upstream, feature should set the upstream as the feature-base
#[test]
fn sets_feature_base_using_remote() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  local.feature(&["start", "topic"]).success();
  let cmd = local
    .git(&["config", "branch.topic.feature-base"])
    .success();
  assert_eq!(get_stdout!(cmd).trim(), "refs/remotes/origin/main");
}

/// Branch names should correctly follow the specified template
#[test]
fn uses_custom_format() {
  let repo = TestRepo::new();
  repo.init_commit();
  repo.write_file(
    "feature.toml",
    r#"[format]
branch_sep = "_"
branch = "%(user)%(sep)%(base)%(sep)%s"
"#,
  );

  // with command line options
  repo
    .feature(&["start", "--format=%(user)/%s", "--sep=-", "new", "branch"])
    .success();

  assert_eq!(repo.get_current_branch(), "test/new-branch");

  // with config file options
  repo.git(&["switch", "main"]).success();
  repo.feature(&["start", "new", "branch"]).success();

  assert_eq!(repo.get_current_branch(), "test_main_new_branch");
}

/// Tests more complex substitutions in the branch name template
#[test]
fn advanced_custom_formats() {
  let repo = TestRepo::new();
  repo.init_commit();

  // success cases
  for (template, expected) in [
    ("feature/%s", "feature/new-branch"),
    ("%(user)/%s", "test/new-branch"),
    ("%(base)%(sep)%s", "main-new-branch"),
    ("%shello", "new-branchhello"),
    ("%%s", "%s"),
    ("%%(user)", "%(user)"),
    ("%%%s", "%new-branch"),
    ("%%%%", "%%"),
    ("", "new-branch"),
  ] {
    repo
      .feature(&["start", &format!("--format={}", template), "new", "branch"])
      .success();

    assert_eq!(repo.get_current_branch(), expected.to_string());
    repo.git(&["switch", "main"]).success();
  }

  // failure cases
  for template in ["%", "%x", "%(what)", "%(use", "%(user", "feature%"] {
    repo
      .feature(&["start", &format!("--format={}", template), "new", "branch"])
      .failure();
  }
}

/// Dry run mode only prints the would-be branch name, and doesn't create or switch to a branch
#[test]
fn dry_run_prints_branch() {
  let repo = TestRepo::new();
  repo.init_commit();
  repo.write_file(
    "feature.toml",
    r#"[format]
branch_sep = "_"
branch = "%(user)%(sep)%(base)%(sep)%s"
"#,
  );

  // with command line options
  let cmd = repo
    .feature(&[
      "start",
      "--dry-run",
      "--format=%(user)/%s",
      "--sep=-",
      "new",
      "branch",
    ])
    .success();

  assert_eq!(
    get_stdout!(cmd).trim(),
    "Created test/new-branch (from main)"
  );

  // with config file options
  // by not switching back to main, we're effectively testing that feature didn't create and switch
  // to the new branch
  let cmd = repo
    .feature(&["start", "--dry-run", "new", "branch"])
    .success();
  assert_eq!(
    get_stdout!(cmd).trim(),
    "Created test_main_new_branch (from main)"
  );
}

/// Should stay on current branch when user specifies --stay
#[test]
fn stays() {
  let repo = TestRepo::new();
  repo.init_commit();
  repo.feature(&["start", "--stay", "topic"]).success();

  let cmd = repo.git(&["branch", "--show-current"]).success();
  assert_eq!(get_stdout!(cmd).trim(), "main", "Should stay on main");

  // combining --stay and --from
  repo.write_file("feature.toml", r#"bases = ["main", "topic"]"#);
  repo
    .feature(&["start", "--stay", "--from", "topic", "subtopic"])
    .success();

  let cmd = repo.git(&["branch", "--show-current"]).success();
  assert_eq!(get_stdout!(cmd).trim(), "main", "Should stay on main");
}
