//! Temp git repositories built by hand, isolated from the host's git config.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A directory under the system temp dir, removed on drop.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let name = format!(
            "build-version-{label}-{}-{}-{nanos}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Panic unless no `.git` sits at or above the temp root: the no-repository rows need it.
pub fn assert_outside_any_repo(dir: &Path) {
    if let Some(found) = dir.ancestors().find(|ancestor| ancestor.join(".git").exists()) {
        panic!(
            "temp dir {} is inside the git repository at {}; point TMPDIR outside any repository",
            dir.display(),
            found.display()
        );
    }
}

/// Run git for fixture setup: host global/system config ignored, signing off, a fixed
/// identity, and no inherited env that selects a repository. Panics on failure.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=build-version tests",
            "-c",
            "user.email=tests@build-version.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_CEILING_DIRECTORIES")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

/// Commit a change to `<name>.txt` with message `name`.
pub fn commit(dir: &Path, name: &str) {
    write(&dir.join(format!("{name}.txt")), name);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", name]);
}

/// Annotated tag on HEAD.
pub fn tag(dir: &Path, name: &str) {
    git(dir, &["tag", "-a", name, "-m", name]);
}

pub fn short_sha(dir: &Path) -> String {
    git(dir, &["rev-parse", "--short=7", "HEAD"])
}

pub fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"])
}

/// The Phase 5.0 matrix: `one` (v0.2.0), `two` (v0.2.1), `three`, `four` on `main`;
/// branch `side` off v0.2.1 with a commit tagged v0.3.0, not reachable from `main`.
pub struct Matrix {
    root: TempDir,
}

impl Matrix {
    pub fn new() -> Self {
        let root = TempDir::new("matrix");
        let repo = root.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        write(
            &repo.join("Cargo.toml"),
            "[package]\nname = \"matrix\"\nversion = \"0.2.1\"\nedition = \"2024\"\n",
        );
        commit(&repo, "one");
        tag(&repo, "v0.2.0");
        commit(&repo, "two");
        tag(&repo, "v0.2.1");
        git(&repo, &["checkout", "-q", "-b", "side"]);
        commit(&repo, "bump-0.3.0");
        tag(&repo, "v0.3.0");
        git(&repo, &["checkout", "-q", "main"]);
        commit(&repo, "three");
        commit(&repo, "four");
        Self { root }
    }

    pub fn repo(&self) -> PathBuf {
        self.root.path().join("repo")
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }
}
