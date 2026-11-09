//! Compile-time `BUILD_VERSION`, `GIT_REVISION`, `GIT_SHA` and `GIT_BRANCH` for a Rust
//! binary, anchored on its Cargo.toml version tag.
//!
//! Call [`emit`] from the consumer's `build.rs`. `BUILD_VERSION` is
//! `git describe --tags --match "v$CARGO_PKG_VERSION" --abbrev=7`: `vX.Y.Z` on the tag,
//! `vX.Y.Z-N-g<sha>` N commits past it, and the bare `vX.Y.Z` before the tag exists.
//! Describe can only name the Cargo.toml version's own tag, never an older one.
//!
//! The cases, checked in order, first match decides (the README carries the table):
//! no repository, git errors, Cargo.toml untracked, shallow clone, tag reaches HEAD,
//! tag not an ancestor, tag absent. Every incomplete input in CI fails the build with a
//! message naming its fix; no value is ever `unknown` or empty.
#![deny(clippy::unwrap_used)]
#![deny(dead_code)]
#![deny(unused_variables)]
#![deny(clippy::string_slice)]

mod error;
mod git;
mod stamp;

use std::path::Path;

use error::Error;
use git::Git;
use stamp::{Revision, Stamp};

const GIT_PROGRAM: &str = "git";
const CI_VARS: [&str; 2] = ["CI", "GITHUB_ACTIONS"];
const SHORT_SHA_LEN: &str = "--short=7";

/// Print the `cargo:` lines for the calling package's build script.
///
/// Reads `CARGO_PKG_VERSION` and `CARGO_MANIFEST_DIR` at build-script run time, so the
/// version is the consumer's, never this crate's. No env var overrides the version.
/// On a failing case it prints the reason to stderr and exits 1, which fails the build.
pub fn emit() {
    let manifest_dir = required_env("CARGO_MANIFEST_DIR");
    let pkg_version = required_env("CARGO_PKG_VERSION");
    let is_ci = CI_VARS.iter().any(|name| is_set(name));
    match resolve(Path::new(&manifest_dir), &pkg_version, is_ci) {
        Ok(stamp) => {
            for line in stamp.lines() {
                println!("{line}");
            }
        }
        Err(err) => fail(&err.to_string()),
    }
}

fn required_env(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => value,
        _ => fail(&format!(
            "{name} is not set: call build_version::emit() from a build.rs that cargo runs"
        )),
    }
}

fn is_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn fail(message: &str) -> ! {
    eprintln!("error: build-version: {message}");
    std::process::exit(1)
}

/// Decide the stamp for the crate at `manifest_dir` whose Cargo.toml version is
/// `pkg_version`, walking the cases table in order.
pub(crate) fn resolve(manifest_dir: &Path, pkg_version: &str, is_ci: bool) -> Result<Stamp, Error> {
    resolve_with(&Git::new(GIT_PROGRAM, manifest_dir), manifest_dir, pkg_version, is_ci)
}

fn resolve_with(git: &Git, manifest_dir: &Path, pkg_version: &str, is_ci: bool) -> Result<Stamp, Error> {
    let tag = format!("v{pkg_version}");
    let dir = manifest_dir.display().to_string();

    // Rows 1 and 2: no repository at all, decided by a directory walk with no git call.
    if !manifest_dir.ancestors().any(|ancestor| ancestor.join(".git").exists()) {
        if is_ci {
            return Err(Error::NoRepoInCi { manifest_dir: dir });
        }
        let warning = format!("no git repository at or above {dir}: BUILD_VERSION is the bare {tag}");
        return Ok(Stamp::bare(tag, warning));
    }

    // Row 3: `.git` is there but git cannot use it.
    let git_dir = git.run(&["rev-parse", "--git-dir"])?;
    if !git_dir.ok {
        return Err(Error::GitDir {
            manifest_dir: dir,
            stderr: git_dir.stderr,
        });
    }

    // Row 4: the enclosing repository does not track this crate (a packaged crate).
    if !git.run(&["ls-files", "--error-unmatch", "Cargo.toml"])?.ok {
        if is_ci {
            return Err(Error::UntrackedInCi { manifest_dir: dir });
        }
        let warning = format!(
            "Cargo.toml in {dir} is not tracked by the enclosing git repository: BUILD_VERSION is the bare {tag}"
        );
        return Ok(Stamp::bare(tag, warning));
    }

    // Row 5: checked before describe, even when HEAD is the tag.
    if git.value(&["rev-parse", "--is-shallow-repository"])? == "true" {
        return Err(Error::Shallow);
    }

    // Rows 6, 7 and 8.
    let version = describe(git, &tag)?;

    let revision = Revision {
        revision: git.value(&["rev-parse", "HEAD"])?,
        sha: git.value(&["rev-parse", SHORT_SHA_LEN, "HEAD"])?,
        branch: git.value(&["rev-parse", "--abbrev-ref", "HEAD"])?,
    };
    let rerun = rerun_paths(git, manifest_dir, &git_dir.stdout)?;
    Ok(Stamp::described(version, revision, rerun))
}

/// Rows 6-8: describe anchored on the Cargo.toml version's own tag.
fn describe(git: &Git, tag: &str) -> Result<String, Error> {
    let described = git.run(&["describe", "--tags", "--match", tag, "--abbrev=7"])?;
    if described.ok {
        if described.stdout.is_empty() {
            return Err(Error::Git {
                command: format!("git describe --tags --match {tag} --abbrev=7"),
                stderr: "printed nothing".to_string(),
            });
        }
        return Ok(described.stdout);
    }
    let tag_ref = format!("refs/tags/{tag}");
    if git.run(&["rev-parse", "-q", "--verify", &tag_ref])?.ok {
        return Err(Error::TagNotAncestor {
            tag: tag.to_string(),
            stderr: described.stderr,
        });
    }
    Ok(tag.to_string())
}

/// HEAD lives in the per-worktree git dir; refs and packed-refs live in the common dir.
/// Relative paths from git are relative to the manifest dir, which is also the base
/// cargo resolves a relative `rerun-if-changed` path against.
fn rerun_paths(git: &Git, manifest_dir: &Path, git_dir: &str) -> Result<Vec<String>, Error> {
    let common_dir = git.value(&["rev-parse", "--git-common-dir"])?;
    let mut paths = vec![format!("{git_dir}/HEAD"), format!("{common_dir}/refs")];
    // A missing rerun path makes cargo rerun the script, and rebuild the crate, on every
    // build. A repo with no packed-refs yet loses nothing: `git pack-refs` also moves
    // loose refs out of `refs/`, which is watched.
    let packed_refs = format!("{common_dir}/packed-refs");
    if manifest_dir.join(&packed_refs).exists() {
        paths.push(packed_refs);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests;
