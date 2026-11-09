//! One test per row of the cases table (design doc Phase 5), against temp repositories.
#![allow(clippy::unwrap_used)]

mod consumer;
mod fixture;

use std::path::Path;

use super::*;
use fixture::{Matrix, TempDir, assert_outside_any_repo, commit, git, head, short_sha, write};

fn version(dir: &Path, pkg_version: &str) -> String {
    resolve(dir, pkg_version, false).unwrap().version
}

fn error(dir: &Path, pkg_version: &str, is_ci: bool) -> String {
    match resolve(dir, pkg_version, is_ci) {
        Ok(stamp) => panic!("expected the build to fail, got BUILD_VERSION={}", stamp.version),
        Err(err) => err.to_string(),
    }
}

#[test]
fn row1_no_repository_outside_ci_is_bare_version_with_warning() {
    let dir = TempDir::new("row1");
    assert_outside_any_repo(dir.path());
    let stamp = resolve(dir.path(), "0.2.1", false).unwrap();
    assert_eq!(stamp.version, "v0.2.1");
    assert!(stamp.revision.is_none());
    let lines = stamp.lines();
    assert!(lines.contains(&"cargo:rustc-env=BUILD_VERSION=v0.2.1".to_string()));
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("cargo:warning=no git repository"))
    );
    assert!(!lines.iter().any(|line| line.contains("GIT_REVISION=")));
}

#[test]
fn row2_no_repository_in_ci_fails() {
    let dir = TempDir::new("row2");
    assert_outside_any_repo(dir.path());
    let message = error(dir.path(), "0.2.1", true);
    assert!(
        message.starts_with("no git repository in CI: check out with git installed and fetch-depth: 0"),
        "{message}"
    );
}

#[test]
fn row3_unusable_git_dir_fails_with_stderr_and_names_safe_directory() {
    let dir = TempDir::new("row3");
    assert_outside_any_repo(dir.path());
    let missing = dir.path().join("nowhere");
    write(&dir.path().join(".git"), &format!("gitdir: {}\n", missing.display()));
    for is_ci in [false, true] {
        let message = error(dir.path(), "0.2.1", is_ci);
        // git's own wording varies by version (2.53: "not a git repository",
        // 2.56: "gitfile does not point to a valid repository"), so pin ours
        // and only require that git's fatal line is passed through.
        assert!(message.starts_with("git rev-parse --git-dir failed in"), "{message}");
        assert!(message.contains("fatal: "), "{message}");
        assert!(message.contains("safe.directory"), "{message}");
    }
}

#[test]
fn row3_missing_git_binary_fails() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    let git = Git::new("build-version-no-such-git", &repo);
    let message = match resolve_with(&git, &repo, "0.2.1", false) {
        Ok(stamp) => panic!("expected failure, got {}", stamp.version),
        Err(err) => err.to_string(),
    };
    assert!(
        message.starts_with("could not run build-version-no-such-git"),
        "{message}"
    );
    assert!(message.contains("install git"), "{message}");
}

#[test]
fn row4_untracked_cargo_toml_outside_ci_is_bare_version_with_warning() {
    let matrix = Matrix::new();
    let crate_dir = matrix.repo().join("vendor").join("packaged");
    write(&crate_dir.join("Cargo.toml"), "[package]\nname = \"packaged\"\n");
    let stamp = resolve(&crate_dir, "1.4.0", false).unwrap();
    assert_eq!(stamp.version, "v1.4.0");
    assert!(stamp.revision.is_none());
    assert!(stamp.warning.unwrap().contains("not tracked"));
}

#[test]
fn row4_untracked_cargo_toml_in_ci_fails() {
    let matrix = Matrix::new();
    let crate_dir = matrix.repo().join("vendor").join("packaged");
    write(&crate_dir.join("Cargo.toml"), "[package]\nname = \"packaged\"\n");
    let message = error(&crate_dir, "1.4.0", true);
    assert!(message.contains("not tracked"), "{message}");
    assert!(message.contains("fetch-depth: 0"), "{message}");
}

#[test]
fn row5_shallow_clone_fails_even_at_the_tag() {
    let matrix = Matrix::new();
    let clone = matrix.root().join("shallow");
    let source = format!("file://{}", matrix.repo().display());
    git(
        matrix.root(),
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            "--branch",
            "v0.2.1",
            &source,
            clone.to_str().unwrap(),
        ],
    );
    // Describe alone would succeed here: the clone holds the tag and HEAD is on it.
    assert_eq!(git(&clone, &["describe", "--tags", "--match", "v0.2.1"]), "v0.2.1");
    for is_ci in [false, true] {
        assert_eq!(
            error(&clone, "0.2.1", is_ci),
            "shallow clone: check out with fetch-depth: 0"
        );
    }
}

#[test]
fn row6_head_at_the_tag_is_the_tag() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    git(&repo, &["checkout", "-q", "v0.2.1"]);
    assert_eq!(version(&repo, "0.2.1"), "v0.2.1");
}

#[test]
fn row6_commits_past_the_tag_carry_count_and_sha() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    assert_eq!(version(&repo, "0.2.1"), format!("v0.2.1-2-g{}", short_sha(&repo)));
}

#[test]
fn row6_describe_names_only_the_cargo_toml_version_tag() {
    // v0.2.1 is the nearest tag; an unanchored describe would name it.
    let matrix = Matrix::new();
    let repo = matrix.repo();
    assert_eq!(version(&repo, "0.2.0"), format!("v0.2.0-3-g{}", short_sha(&repo)));
}

#[test]
fn row7_tag_not_an_ancestor_fails_with_rebase_on_main() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    for is_ci in [false, true] {
        let message = error(&repo, "0.3.0", is_ci);
        assert!(message.contains("No tags can describe"), "{message}");
        assert!(
            message.ends_with("tag v0.3.0 exists but is not an ancestor of HEAD: rebase on main"),
            "{message}"
        );
    }
}

#[test]
fn row8_tag_absent_is_bare_version() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    let stamp = resolve(&repo, "0.2.2", true).unwrap();
    assert_eq!(stamp.version, "v0.2.2");
    assert!(stamp.warning.is_none());
    assert_eq!(stamp.revision.unwrap().revision, head(&repo));
}

#[test]
fn described_stamp_sets_every_value_and_watches_head_refs_and_packed_refs() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    git(&repo, &["pack-refs", "--all"]);
    let lines = resolve(&repo, "0.2.1", false).unwrap().lines();
    let short = short_sha(&repo);
    let expected = [
        "cargo:rerun-if-env-changed=CI".to_string(),
        "cargo:rerun-if-env-changed=GITHUB_ACTIONS".to_string(),
        "cargo:rerun-if-changed=.git/HEAD".to_string(),
        "cargo:rerun-if-changed=.git/refs".to_string(),
        "cargo:rerun-if-changed=.git/packed-refs".to_string(),
        format!("cargo:rustc-env=BUILD_VERSION=v0.2.1-2-g{short}"),
        format!("cargo:rustc-env=GIT_REVISION={}", head(&repo)),
        format!("cargo:rustc-env=GIT_SHA={short}"),
        "cargo:rustc-env=GIT_BRANCH=main".to_string(),
    ];
    assert_eq!(lines, expected);
}

#[test]
fn missing_packed_refs_is_not_watched() {
    let dir = TempDir::new("loose");
    assert_outside_any_repo(dir.path());
    git(dir.path(), &["init", "-q"]);
    write(&dir.path().join("Cargo.toml"), "[package]\n");
    commit(dir.path(), "one");
    assert!(!dir.path().join(".git/packed-refs").exists());
    let stamp = resolve(dir.path(), "0.1.0", false).unwrap();
    assert_eq!(stamp.rerun, vec![".git/HEAD".to_string(), ".git/refs".to_string()]);
}

#[test]
fn worktree_watches_its_own_head_and_the_common_refs() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    let worktree = matrix.root().join("wt");
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "wt", worktree.to_str().unwrap(), "main"],
    );
    let stamp = resolve(&worktree, "0.2.1", false).unwrap();
    assert_eq!(stamp.version, format!("v0.2.1-2-g{}", short_sha(&worktree)));
    let common = git(&worktree, &["rev-parse", "--git-common-dir"]);
    assert!(stamp.rerun[0].ends_with("/worktrees/wt/HEAD"), "{:?}", stamp.rerun);
    assert_eq!(stamp.rerun[1], format!("{common}/refs"));
}

#[test]
fn detached_head_reports_head_as_branch() {
    let matrix = Matrix::new();
    let repo = matrix.repo();
    git(&repo, &["checkout", "-q", "--detach"]);
    assert_eq!(resolve(&repo, "0.2.1", false).unwrap().revision.unwrap().branch, "HEAD");
}

#[test]
fn unborn_head_fails_instead_of_an_empty_revision() {
    let dir = TempDir::new("unborn");
    assert_outside_any_repo(dir.path());
    git(dir.path(), &["init", "-q"]);
    write(&dir.path().join("Cargo.toml"), "[package]\n");
    git(dir.path(), &["add", "Cargo.toml"]);
    let message = error(dir.path(), "0.1.0", false);
    assert!(message.starts_with("git rev-parse HEAD failed"), "{message}");
}
