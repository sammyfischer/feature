use crate::common::TestRepo;

mod common;

fn add_file(repo: &TestRepo) {
  let file_name = "file.txt";
  repo.write_file(file_name, "hello world");
  repo.git(&["add", &file_name]).success();
}

#[test]
fn commits() {
  let repo = TestRepo::new();

  // create and add file
  add_file(&repo);

  // commit it
  repo.feature(&["commit", "initial", "commit"]).success();

  // check latest commit message
  let proc = repo.git(&["log", "-1", "--pretty=%B"]).success();
  let Ok(stdout) = String::from_utf8(proc.get_output().stdout.clone()) else {
    panic!("Failed to get stdout as string")
  };
  assert_eq!(stdout.trim(), "initial commit".to_string());
}

#[test]
fn no_message_fails() {
  let repo = TestRepo::new();
  add_file(&repo);

  repo.feature(&["commit", ""]).failure();
}

/// Committing with a failing pre-commit script should not go through
#[test]
fn pre_commit_can_fail() {
  let repo = TestRepo::new();
  // hook that always fails
  repo.create_precommit_hook("false");

  add_file(&repo);
  repo
    .feature(&["commit", "this", "should", "fail"])
    .failure();

  // check that there are no commits
  let proc = repo.git(&["log", "--oneline"]).failure();
  let text = String::from_utf8(proc.get_output().stderr.clone()).expect("Output should exist");
  assert_eq!(
    text.trim(),
    "fatal: your current branch 'main' does not have any commits yet"
  )
}

#[test]
fn pre_commit_no_verify_passes() {
  let repo = TestRepo::new();
  // hook that always fails
  repo.create_precommit_hook("false");

  add_file(&repo);
  repo
    .feature(&["commit", "--no-verify", "this", "should", "succeed"])
    .success();
}
