//! A real consumer crate built by cargo: `build.rs` calls `build_version::emit()` and
//! `main` prints what it compiled in. Proves `emit()` reads the consumer's version at
//! build-script run time and that env never overrides the version or the repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::fixture::{TempDir, assert_outside_any_repo, commit, git, head, short_sha, tag, write};

const CONSUMER_VERSION: &str = "3.4.5";

const BUILD_RS: &str = "fn main() {\n    build_version::emit();\n}\n";

const MAIN_RS: &str = r#"fn main() {
    println!("BUILD_VERSION={}", env!("BUILD_VERSION"));
    println!("GIT_REVISION={}", env!("GIT_REVISION"));
    println!("GIT_SHA={}", env!("GIT_SHA"));
    println!("GIT_BRANCH={}", env!("GIT_BRANCH"));
}
"#;

fn this_crate() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A consumer repo at `CONSUMER_VERSION`, tagged, with one commit past the tag.
fn consumer_repo(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    git(root, &["init", "-q", repo.to_str().unwrap()]);
    let manifest = format!(
        "[package]\nname = \"consumer\"\nversion = \"{CONSUMER_VERSION}\"\nedition = \"2024\"\nbuild = \"build.rs\"\n\n\
         [build-dependencies]\nbuild-version = {{ path = \"{}\" }}\n",
        this_crate().display()
    );
    write(&repo.join("Cargo.toml"), &manifest);
    write(&repo.join("build.rs"), BUILD_RS);
    write(&repo.join("src/main.rs"), MAIN_RS);
    // Build with the toolchain this crate pins, not whatever rustup defaults to.
    let toolchain = std::fs::read_to_string(this_crate().join("rust-toolchain.toml")).unwrap();
    write(&repo.join("rust-toolchain.toml"), &toolchain);
    write(&repo.join(".gitignore"), "/Cargo.lock\n");
    commit(&repo, "consumer");
    tag(&repo, &format!("v{CONSUMER_VERSION}"));
    commit(&repo, "past-the-tag");
    repo
}

/// `cargo run` the consumer with `env` added; return its stdout.
fn run(repo: &Path, target: &Path, env: &[(&str, &str)]) -> String {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--offline"])
        .current_dir(repo)
        .env("CARGO_TARGET_DIR", target)
        .envs(env.iter().copied())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "consumer build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn expected(repo: &Path) -> String {
    format!(
        "BUILD_VERSION=v{CONSUMER_VERSION}-1-g{}\nGIT_REVISION={}\nGIT_SHA={}\nGIT_BRANCH=main\n",
        short_sha(repo),
        head(repo),
        short_sha(repo)
    )
}

#[test]
fn consumer_gets_its_own_version_not_build_versions() {
    assert_ne!(env!("CARGO_PKG_VERSION"), CONSUMER_VERSION);
    let root = TempDir::new("consumer");
    assert_outside_any_repo(root.path());
    let repo = consumer_repo(root.path(), "consumer");
    let output = run(&repo, &root.path().join("target"), &[]);
    assert_eq!(output, expected(&repo));
}

#[test]
fn env_version_overrides_are_ignored() {
    let root = TempDir::new("override");
    assert_outside_any_repo(root.path());
    let repo = consumer_repo(root.path(), "consumer");
    let output = run(
        &repo,
        &root.path().join("target"),
        &[("GIT_DESCRIBE", "v9.9.9"), ("BUILD_VERSION", "v9.9.9")],
    );
    assert_eq!(output, expected(&repo));
}

#[test]
fn inherited_git_dir_cannot_point_describe_at_another_repo() {
    let root = TempDir::new("gitdir");
    assert_outside_any_repo(root.path());
    let repo = consumer_repo(root.path(), "consumer");
    // Same tag name, a different history: two commits past, so any leak shows.
    let other = consumer_repo(root.path(), "other");
    commit(&other, "other-extra");
    let other_git = other.join(".git");
    let output = run(
        &repo,
        &root.path().join("target"),
        &[
            ("GIT_DIR", other_git.to_str().unwrap()),
            ("GIT_WORK_TREE", other.to_str().unwrap()),
            ("GIT_COMMON_DIR", other_git.to_str().unwrap()),
            ("GIT_INDEX_FILE", other_git.join("index").to_str().unwrap()),
            ("GIT_CEILING_DIRECTORIES", root.path().to_str().unwrap()),
        ],
    );
    assert_eq!(output, expected(&repo));
}
