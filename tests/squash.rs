use crate::common::TestRepo;

mod common;

#[test]
fn squashes_commits() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();

  for change in ["B", "C", "D"] {
    repo.write_file(file_name, &format!("{}\n", change));
    repo.commit_all(change);
  }

  repo.feature(&["squash"]).success();

  repo
    .git(&["show", "--no-patch", "--pretty=format:%B"])
    .success()
    .stdout(
      r#"Squash topic onto main

* D

* C

* B"#,
    );

  // check contents of file in commit
  repo
    .git(&["show", &format!("HEAD:{}", file_name)])
    .success()
    .stdout("D\n");
}

/// Feature should add co-author footers when some of the commits have different
/// authors
#[test]
fn adds_co_authors() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();

  repo.set_user("Test 2", "test2@test.com");
  repo.write_file(file_name, "B\n");
  repo.commit_all("B");

  repo.set_user("Test 3", "test3@test.com");
  repo.write_file(file_name, "C\n");
  repo.commit_all("C");

  repo.set_user("Test", "test@test.com");
  repo.feature(&["squash"]).success();

  repo
    .git(&["show", "--no-patch", "--pretty=format:%B"])
    .success()
    .stdout(
      r#"Squash topic onto main

* C

* B

Co-authored-by: Test 3 <test3@test.com>
Co-authored-by: Test 2 <test2@test.com>"#,
    );
}
