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
  let cmd = repo.git(&["log", "-1", "--pretty=%B"]).success();
  assert_eq!(get_stdout!(cmd).trim(), "initial commit");
}

#[test]
fn no_message_fails() {
  let repo = TestRepo::new();
  add_file(&repo);

  repo.feature(&["commit", ""]).failure();
}

/// Should fail if there are no staged changes
#[test]
fn fails_on_empty_index() {
  let repo = TestRepo::new();
  repo.init_commit();
  repo.feature(&["commit", "nothing"]).failure();
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
  let cmd = repo.git(&["log", "--oneline"]).failure();
  assert_eq!(
    get_stderr!(cmd).trim(),
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

/// Committing during a merge conflict should correctly set both commit parents
#[test]
fn merge_commit_has_both_parents() {
  let repo = TestRepo::new();
  let file_name = "file.txt";
  repo.write_file(file_name, "A");
  repo.commit_all("A");

  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "C");
  repo.commit_all("C");

  repo.git(&["switch", "main"]).success();
  repo.write_file(file_name, "B");
  repo.commit_all("B");

  // where main points before merge
  let cmd = repo.git(&["rev-parse", "main"]).success();
  let main_hash = get_stdout!(cmd);
  let main_hash = main_hash.trim();

  // where topic points before merge
  let cmd = repo.git(&["rev-parse", "topic"]).success();
  let topic_hash = get_stdout!(cmd);
  let topic_hash = topic_hash.trim();

  repo.git(&["switch", "topic"]).success();
  repo.git(&["merge", "main"]).failure();

  repo.write_file(file_name, "BC");
  repo.git(&["add", file_name]).success();
  repo
    .feature(&["commit", "Merged main into topic"])
    .success();

  // first parent of merge commit
  let cmd = repo.git(&["rev-parse", "HEAD^1"]).success();
  let parent1 = get_stdout!(cmd);
  let parent1 = parent1.trim();

  // second parent of merge commit
  let cmd = repo.git(&["rev-parse", "HEAD^2"]).success();
  let parent2 = get_stdout!(cmd);
  let parent2 = parent2.trim();

  assert_eq!(
    topic_hash, parent1,
    "First parent should point to the commit from topic"
  );
  assert_eq!(
    main_hash, parent2,
    "Second parent should point to the commit from main"
  );
}

/// When no commit message is specified, default to MERGE_MSG if a merge is active
#[test]
fn merge_commit_uses_merge_msg() {
  let repo = TestRepo::new();
  let file_name = "file.txt";
  repo.write_file(file_name, "A");
  repo.commit_all("A");

  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "C");
  repo.commit_all("C");

  repo.git(&["switch", "main"]).success();
  repo.write_file(file_name, "B");
  repo.commit_all("B");

  repo.git(&["switch", "topic"]).success();
  repo.git(&["merge", "main"]).failure();

  repo.write_file(file_name, "BC");
  repo.git(&["add", file_name]).success();

  // commit with no message
  repo.feature(&["commit"]).success();

  let cmd = repo.git(&["show", "HEAD", "--no-patch", "--pretty=format:%s"]);
  // the actual message may depend on git config, but starts with should be pretty good
  assert!(
    get_stdout!(cmd)
      .trim()
      .starts_with("Merge branch 'main' into topic")
  );
}
