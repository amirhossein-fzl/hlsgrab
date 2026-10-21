use std::{ffi::OsString, process::Stdio};

use tokio::process::Command;

use crate::merge::errors::MergeError;

#[derive(Debug)]
pub struct ProcessOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait ProcessRunner: Sync {
    fn run<'a>(
        &'a self,
        binary: &'static str,
        args: &'a [OsString],
    ) -> impl Future<Output = Result<ProcessOutput, MergeError>> + Send + 'a;
}

pub struct TokioRunner;

impl ProcessRunner for TokioRunner {
    fn run<'a>(
        &'a self,
        binary: &'static str,
        args: &'a [OsString],
    ) -> impl Future<Output = Result<ProcessOutput, MergeError>> + Send + 'a {
        async move {
            let output = Command::new(binary)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output()
                .await
                .map_err(|source| MergeError::Spawn { binary, source })?;

            Ok(ProcessOutput {
                code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            })
        }
    }
}
