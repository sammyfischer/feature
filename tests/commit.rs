use crate::common::{Hook, TestRepo};

mod common;

fn add_file(repo: &TestRepo) {
  let file_name = "file.txt";
  repo.write_file(file_name, "hello world");
  repo.git(&["add", file_name]).success();
}

/// Feature should be able to commit to an empty repository
#[test]
fn commits_initial_commit() {
  let repo = TestRepo::new();

  // create and add file
  add_file(&repo);

  // commit it
  let cmd = repo.feature(&["commit", "initial", "commit"]).success();
  println!("{}", get_stdout!(cmd));

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

/// Specifying --to <branch> should commit to that branch
#[test]
fn commits_to_target_branch() {
  let repo = TestRepo::new();
  let file_name = "file.txt";
  // commit A to main
  repo.write_file(file_name, "A\n");
  repo.commit_all("A");

  // commit B to topic
  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "B\n");
  repo.commit_all("B");

  // commit X to topic2 from topic (in a different file)
  repo
    .feature(&["start", "--stay", "--from", "main", "topic2"])
    .success();
  repo.write_file("file2.txt", "X\n");
  repo.git(&["add", "file2.txt"]).success();
  repo.feature(&["commit", "--to", "topic2", "X"]).success();

  // commits ended up in the right place
  assert_eq!(
    repo.list_commit_subjects("topic2"),
    "X\nA",
    "Commit X should have gone to topic2"
  );
  assert_eq!(
    repo.list_commit_subjects("topic"),
    "B\nA",
    "Commit X should not have gone to topic"
  );

  // commits contain correct changes
  let cmd = repo
    .git(&["show", &format!("topic2:{}", "file2.txt")])
    .success();
  assert_eq!(get_stdout!(cmd), "X\n", "topic2 contains the wrong changes");

  let cmd = repo
    .git(&["show", &format!("topic:{}", file_name)])
    .success();
  assert_eq!(get_stdout!(cmd), "B\n", "topic contains the wrong changes");
}

/// Feature should run the prepare-commit-msg hook
#[test]
fn hook_prepare_msg() {
  let repo = TestRepo::new();
  repo.install_hook(
    Hook::PrepareMsg,
    include_str!("./hooks/prepare-commit-msg.sh"),
  );

  repo.write_file("file.txt", "A\n");
  repo.git(&["add", "file.txt"]).success();
  repo.feature(&["commit", "A"]).success();

  assert_eq!(repo.list_commit_subjects("main"), "from command line: A");

  repo.install_hook(Hook::PrepareMsg, include_str!("./hooks/fail.sh"));
  repo.write_file("file.txt", "B\n");
  repo.git(&["add", "file.txt"]).success();

  repo.feature(&["commit", "B"]).failure().stderr(
    r#"prepare-commit-msg hook failed!

Error: prepare-commit-msg hook failed
"#,
  );

  // still runs with --no-verify, and can fail
  repo
    .feature(&["commit", "--no-verify", "B"])
    .failure()
    .stderr(
      r#"prepare-commit-msg hook failed!

Error: prepare-commit-msg hook failed
"#,
    );
}

/// Feature should run the commit-msg hook
#[test]
fn hook_commit_msg() {
  let repo = TestRepo::new();
  repo.install_hook(Hook::CommitMsg, include_str!("./hooks/commit-msg.sh"));

  repo.write_file("file.txt", "A\n");
  repo.git(&["add", "file.txt"]).success();
  repo.feature(&["commit", "A"]).success();

  assert_eq!(repo.list_commit_subjects("main"), "fix: A");

  repo.install_hook(Hook::CommitMsg, include_str!("./hooks/fail.sh"));
  repo.write_file("file.txt", "B\n");
  repo.git(&["add", "file.txt"]).success();

  repo.feature(&["commit", "B"]).failure().stderr(
    r#"commit-msg hook failed!

Error: commit-msg hook failed
"#,
  );

  // bypassed with --no-verify
  repo.feature(&["commit", "--no-verify", "B"]).success();

  assert_eq!(repo.list_commit_subjects("main"), "B\nfix: A");
}

/// Feature should run the pre-commit hook
#[test]
fn hook_pre_commit() {
  let repo = TestRepo::new();
  // hook that always fails
  repo.install_hook(Hook::PreCommit, include_str!("./hooks/fail.sh"));

  add_file(&repo);
  repo
    .feature(&["commit", "this", "should", "fail"])
    .failure();

  // check that there are no commits
  let cmd = repo.git(&["log", "--oneline"]).failure();
  assert_eq!(
    get_stderr!(cmd).trim(),
    "fatal: your current branch 'main' does not have any commits yet"
  );

  // bypassed with --no-verify
  repo
    .feature(&["commit", "--no-verify", "this", "should", "succeed"])
    .success();
}

/// Feature should run the post-commit hook
#[test]
fn hook_post_commit() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.install_hook(Hook::PostCommit, include_str!("./hooks/post-commit.sh"));

  add_file(&repo);
  repo
    .feature(&["commit", "this", "should", "fail"])
    .success()
    .stderr("Commit succeeded\n");

  repo.install_hook(Hook::PostCommit, include_str!("./hooks/fail.sh"));

  // when post-commit fails, error output is printed but the commit succeeds
  repo.write_file(file_name, "B\n");
  repo.git(&["add", file_name]).success();
  repo
    .feature(&["commit", "B"])
    .success()
    .stderr("post-commit hook failed\n");
}

/// Feature should run the post-rewrite hook
#[test]
fn hook_post_rewrite() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();
  repo.install_hook(Hook::PostRewrite, include_str!("./hooks/post-rewrite.sh"));

  let cmd = repo
    .git(&["show", "--pretty=format:%H", "--no-patch"])
    .success();
  let old_id = get_stdout!(cmd);
  let old_id = old_id.trim();

  repo.write_file(file_name, "A\n");
  let cmd = repo.feature(&["commit", "--amend"]).success();
  let stderr = get_stderr!(cmd);
  assert!(
    stderr.starts_with(old_id),
    "Output should start with '{}', instead got:\n{}",
    old_id,
    stderr
  );

  repo.install_hook(Hook::PostRewrite, include_str!("./hooks/fail.sh"));

  // when post-rewrite fails, error output is printed but the commit succeeds
  repo.write_file(file_name, "B\n");
  repo.git(&["add", file_name]).success();
  repo
    .feature(&["commit", "--amend"])
    .success()
    .stderr("post-rewrite hook failed\n");
}
