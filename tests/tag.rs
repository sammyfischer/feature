use crate::common::TestRepo;

mod common;

#[test]
fn creates_and_pushes_tag() {
  let (local, remote) = TestRepo::new_with_remote();

  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // create and push tag
  local.feature(&["tag", "1.0.0"]).success();

  // tag points to the correct commit
  remote
    .git(&["show", "--no-patch", "--format=%s", "v1.0.0"])
    .success()
    .stdout("A\n");
}

/// Should create the tag at the specified target
#[test]
fn uses_correct_target() {
  let (local, remote) = TestRepo::new_with_remote();

  local.init_commit();
  local.write_file("file.txt", "B\n");
  local.commit_all("B");
  local.git(&["push", "-u", "origin", "main"]).success();

  // create and push tag at A, not B
  local.feature(&["tag", "--at", "HEAD^1", "1.0.0"]).success();

  // tag points to the correct commit
  remote
    .git(&["show", "--no-patch", "--format=%s", "v1.0.0"])
    .success()
    .stdout("A\n");
}

/// Should accept a leading 'v' in the semver string
#[test]
fn accepts_leading_v() {
  let (local, remote) = TestRepo::new_with_remote();

  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // create and push tag
  local.feature(&["tag", "v1.0.0"]).success();

  // tag points to the correct commit
  remote
    .git(&["show", "--no-patch", "--format=%s", "v1.0.0"])
    .success()
    .stdout("A\n");
}

/// Should create an annotated tag when a message is specified
#[test]
fn creates_annotated_tag() {
  let (local, remote) = TestRepo::new_with_remote();

  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // create and push tag
  local
    .feature(&["tag", "-m", "Release v1.0.0", "1.0.0"])
    .success();

  // tag is an annotated tag. if it was a lightweight tag, it would print the
  // commit message instead of the tag message
  remote
    .git(&["tag", "--list", "--format=%(contents)", "v1.0.0"])
    .success()
    .stdout("Release v1.0.0\n");
}

/// When configured, feature should require annotated tags
#[test]
fn requires_annotated_tag() {
  let (local, remote) = TestRepo::new_with_remote();
  local.write_file(
    "feature.toml",
    r#"[tag]
require_annotated = true
"#,
  );

  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // create lightweight tag
  local.feature(&["tag", "1.0.0"]).failure();

  // local doesn't have tag
  local
    .git(&["tag", "--list"])
    .success()
    .stdout("")
    .stderr("");

  // remote doesn't have tag
  remote
    .git(&["tag", "--list"])
    .success()
    .stdout("")
    .stderr("");
}
