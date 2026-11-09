use std::fmt;

/// A case in which the build must fail. Each message names its fix.
#[derive(Debug)]
pub(crate) enum Error {
    /// Row 2: no `.git` at or above the manifest dir, in CI.
    NoRepoInCi { manifest_dir: String },
    /// Row 4 in CI: the enclosing repository does not track this crate's Cargo.toml.
    UntrackedInCi { manifest_dir: String },
    /// Row 3: the git binary could not be started.
    GitSpawn { program: String, reason: String },
    /// Row 3: `git rev-parse --git-dir` failed (dubious ownership, a broken `.git`).
    GitDir { manifest_dir: String, stderr: String },
    /// A later git call failed or printed nothing (an unborn HEAD, a corrupt repo).
    Git { command: String, stderr: String },
    /// Row 5: shallow clone.
    Shallow,
    /// Row 7: the tag exists but HEAD does not descend from it.
    TagNotAncestor { tag: String, stderr: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRepoInCi { manifest_dir } => write!(
                f,
                "no git repository in CI: check out with git installed and fetch-depth: 0 \
                 (no .git at or above {manifest_dir})"
            ),
            Self::UntrackedInCi { manifest_dir } => write!(
                f,
                "Cargo.toml in {manifest_dir} is not tracked by the enclosing git repository in CI: \
                 check out the crate's own repository with git installed and fetch-depth: 0"
            ),
            Self::GitSpawn { program, reason } => write!(
                f,
                "could not run {program} ({reason}): install git where cargo runs this build"
            ),
            Self::GitDir { manifest_dir, stderr } => write!(
                f,
                "git rev-parse --git-dir failed in {manifest_dir}: {stderr}\n\
                 if git reports dubious ownership, add the checkout to safe.directory: \
                 git config --global --add safe.directory <repo root>"
            ),
            Self::Git { command, stderr } => write!(f, "{command} failed: {stderr}"),
            Self::Shallow => write!(f, "shallow clone: check out with fetch-depth: 0"),
            Self::TagNotAncestor { tag, stderr } => write!(
                f,
                "{stderr}\ntag {tag} exists but is not an ancestor of HEAD: rebase on main"
            ),
        }
    }
}
