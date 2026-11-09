use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::Error;

/// Inherited git env that could point git at another repository than the crate's own.
const SCRUBBED_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_CEILING_DIRECTORIES",
];

/// Runs git in one directory with the repository-selecting env removed.
pub(crate) struct Git {
    program: OsString,
    dir: PathBuf,
}

/// One finished git call: exit status, trimmed stdout and stderr.
pub(crate) struct Run {
    pub(crate) ok: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl Git {
    pub(crate) fn new(program: impl Into<OsString>, dir: &Path) -> Self {
        Self {
            program: program.into(),
            dir: dir.to_path_buf(),
        }
    }

    /// Run git; a non-zero exit is data in [`Run`], only a spawn failure is an error.
    pub(crate) fn run(&self, args: &[&str]) -> Result<Run, Error> {
        let mut command = Command::new(&self.program);
        command.args(args).current_dir(&self.dir);
        for name in SCRUBBED_ENV {
            command.env_remove(name);
        }
        let output = command.output().map_err(|err| Error::GitSpawn {
            program: self.program.to_string_lossy().into_owned(),
            reason: err.to_string(),
        })?;
        Ok(Run {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }

    /// Run git and require success and non-empty output.
    pub(crate) fn value(&self, args: &[&str]) -> Result<String, Error> {
        let run = self.run(args)?;
        if run.ok && !run.stdout.is_empty() {
            return Ok(run.stdout);
        }
        let stderr = if run.stderr.is_empty() { "printed nothing".to_string() } else { run.stderr };
        Err(Error::Git {
            command: format!("git {}", args.join(" ")),
            stderr,
        })
    }
}
