use crate::CI_VARS;

/// The commit-identifying values, present only when the crate's own repository was read.
pub(crate) struct Revision {
    pub(crate) revision: String,
    pub(crate) sha: String,
    pub(crate) branch: String,
}

/// What a build script tells cargo: the version, the commit values when there is a
/// repository, what to watch, and an optional warning.
pub(crate) struct Stamp {
    pub(crate) version: String,
    pub(crate) revision: Option<Revision>,
    pub(crate) rerun: Vec<String>,
    pub(crate) warning: Option<String>,
}

impl Stamp {
    /// Rows 1 and 4 outside CI: no repository of this crate's to read. Only
    /// `BUILD_VERSION` is set; the commit values are left unset rather than faked, so a
    /// consumer that reads them with `env!()` fails to compile instead of baking a lie.
    pub(crate) fn bare(version: String, reason: String) -> Self {
        Self {
            version,
            revision: None,
            rerun: Vec::new(),
            warning: Some(format!("{reason}; GIT_REVISION, GIT_SHA and GIT_BRANCH are not set")),
        }
    }

    /// Rows 6 and 8: the crate's repository was read.
    pub(crate) fn described(version: String, revision: Revision, rerun: Vec<String>) -> Self {
        Self {
            version,
            revision: Some(revision),
            rerun,
            warning: None,
        }
    }

    /// The `cargo:` lines, in print order.
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = CI_VARS
            .iter()
            .map(|name| format!("cargo:rerun-if-env-changed={name}"))
            .collect();
        lines.extend(self.rerun.iter().map(|path| format!("cargo:rerun-if-changed={path}")));
        lines.push(format!("cargo:rustc-env=BUILD_VERSION={}", self.version));
        if let Some(revision) = &self.revision {
            lines.push(format!("cargo:rustc-env=GIT_REVISION={}", revision.revision));
            lines.push(format!("cargo:rustc-env=GIT_SHA={}", revision.sha));
            lines.push(format!("cargo:rustc-env=GIT_BRANCH={}", revision.branch));
        }
        if let Some(warning) = &self.warning {
            lines.push(format!("cargo:warning={warning}"));
        }
        lines
    }
}
