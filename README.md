# build-version-rs

Compile-time `BUILD_VERSION`, `GIT_REVISION`, `GIT_SHA` and `GIT_BRANCH` for Tatari Rust binaries, anchored on the Cargo.toml version tag.

A build on the tag `v0.2.1` prints `v0.2.1`. A build N commits past it prints `v0.2.1-N-g<sha>`. A build of a commit whose Cargo.toml version has no tag yet (a release merge before `bump finish` tags it) prints the bare `v0.2.1`. The version comes from `git describe --tags --match "v$CARGO_PKG_VERSION" --abbrev=7`, so describe can only name the Cargo.toml version's own tag, never an older one.

Tracked under [SRE-4429](https://tatari.atlassian.net/browse/SRE-4429). Design: `tatari-tv/github-actions` `docs/plan/2026-11-08-rust-version-from-cargo-toml.md`, Phase 5.

## Usage

Add it as a build dependency, pinned to a tag:

```toml
[build-dependencies]
build-version = { git = "https://github.com/tatari-tv/build-version-rs", tag = "v0.1.0" }
```

Call it from `build.rs`:

```rust
fn main() {
    build_version::emit();
}
```

Read the values with `env!`:

```rust
#[derive(clap::Parser)]
#[command(version = env!("BUILD_VERSION"))]
struct Cli {}
```

| var | value |
| --- | --- |
| `BUILD_VERSION` | `vX.Y.Z`, `vX.Y.Z-N-g<sha>`, or the bare `vX.Y.Z` before the tag exists |
| `GIT_REVISION` | full commit sha |
| `GIT_SHA` | short commit sha (at least 7 hex) |
| `GIT_BRANCH` | `git rev-parse --abbrev-ref HEAD`: the branch, or `HEAD` when detached |

`cargo:rustc-env` reaches only the package whose build script prints it, so every binary crate in a workspace calls `emit()` from its own `build.rs`.

No value is ever `unknown` or empty. A case that cannot produce a true value fails the build. The two no-repository cases outside CI set `BUILD_VERSION` alone: a consumer reading `GIT_REVISION` with `env!` then fails to compile instead of baking in a placeholder.

No env var overrides the version. `GIT_DESCRIBE` and `BUILD_VERSION` in the build env are ignored, and git runs in `CARGO_MANIFEST_DIR` with `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_COMMON_DIR` and `GIT_CEILING_DIRECTORIES` removed, so an inherited var cannot point describe at another repository.

## Cases

Checked in this order; the first match decides. `$V` is the consumer's `CARGO_PKG_VERSION`. "In CI" means `CI` or `GITHUB_ACTIONS` is set.

| # | case | result |
| --- | --- | --- |
| 1 | no `.git` at or above `CARGO_MANIFEST_DIR`, not in CI | `v$V` and a `cargo:warning` |
| 2 | no `.git`, in CI | build fails: `no git repository in CI: check out with git installed and fetch-depth: 0` |
| 3 | `.git` present but git errors (no `git` binary, dubious ownership, any `rev-parse --git-dir` failure) | build fails with git's stderr; the message names `safe.directory` |
| 4 | `Cargo.toml` not tracked by the enclosing repository (a packaged crate inside a parent repo) | as row 1 outside CI, as row 2 in CI |
| 5 | shallow clone | build fails: `shallow clone: check out with fetch-depth: 0` (checked before describe, even when HEAD is the tag) |
| 6 | tag `v$V` exists and reaches HEAD | the describe output: `v$V` or `v$V-N-g<sha>` |
| 7 | tag `v$V` exists but is not an ancestor of HEAD (a stale branch after a squash merge) | build fails with git's stderr and `rebase on main` |
| 8 | tag `v$V` absent | `v$V` |

A `cargo install --git <repo>` checkout has `.git` and no tags, so it takes row 8 and prints the bare `v$V`. With `--tag vX.Y.Z` it is exact.

## CI: check out with full history

Every job that compiles a consumer (build, test, clippy, mutants) needs the full history and tags, or rows 2, 3 and 5 fail the build:

```yaml
- uses: actions/checkout@v6
  with:
    fetch-depth: 0
```

`tatari-tv/github-actions/checkout@v5` already unshallows and fetches tags.

## Rebuilds

`emit()` prints `rerun-if-changed` for the worktree's `HEAD` and the common dir's `refs` and `packed-refs` (when it exists), plus `rerun-if-env-changed` for `CI` and `GITHUB_ACTIONS`. A new commit, a checkout or a new tag reruns the script. Worktrees resolve their own `HEAD`.

## Development

```bash
otto ci
```

The tests build temp git repositories with annotated tags for every row above, plus a consumer crate that cargo builds and runs.
