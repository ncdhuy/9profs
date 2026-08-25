use std::{ffi::OsString, path::PathBuf, process::Stdio};

use async_trait::async_trait;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

#[derive(Clone, Debug)]
pub(crate) struct ProcessOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Error)]
pub(crate) enum ProcessError {
    #[error("could not start OfficeCLI")]
    Spawn(#[source] std::io::Error),
    #[error("OfficeCLI process failed")]
    Io(#[source] std::io::Error),
    #[error("OfficeCLI output reader failed")]
    Reader,
}

#[async_trait]
pub(crate) trait ProcessBackend: Send + Sync {
    async fn run(
        &self,
        args: &[OsString],
        environment: &[(String, OsString)],
        max_output_bytes: usize,
    ) -> Result<ProcessOutput, ProcessError>;
}

#[derive(Clone, Debug)]
pub(crate) struct SubprocessBackend {
    program: PathBuf,
}

impl SubprocessBackend {
    pub(crate) fn new(program: PathBuf) -> Self {
        Self { program }
    }
}

#[async_trait]
impl ProcessBackend for SubprocessBackend {
    async fn run(
        &self,
        args: &[OsString],
        environment: &[(String, OsString)],
        max_output_bytes: usize,
    ) -> Result<ProcessOutput, ProcessError> {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .env_clear()
            .envs(environment.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(ProcessError::Spawn)?;
        let stdout = child.stdout.take().ok_or(ProcessError::Reader)?;
        let stderr = child.stderr.take().ok_or(ProcessError::Reader)?;
        let stdout_task = tokio::spawn(read_limited(stdout, max_output_bytes));
        let stderr_task = tokio::spawn(read_limited(stderr, max_output_bytes));
        let status = child.wait().await.map_err(ProcessError::Io)?;
        let stdout = stdout_task.await.map_err(|_| ProcessError::Reader)??;
        let stderr = stderr_task.await.map_err(|_| ProcessError::Reader)??;
        Ok(ProcessOutput {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code: status.code(),
        })
    }
}

async fn read_limited<R>(mut reader: R, max_output_bytes: usize) -> Result<Vec<u8>, ProcessError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::with_capacity(max_output_bytes.min(8192));
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).await.map_err(ProcessError::Io)?;
        if count == 0 {
            break;
        }
        let remaining = max_output_bytes.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt, duplex};

    use super::read_limited;

    #[tokio::test]
    async fn output_reader_caps_captured_bytes() {
        let (mut writer, reader) = duplex(64);
        writer.write_all(b"0123456789").await.unwrap();
        writer.shutdown().await.unwrap();

        assert_eq!(read_limited(reader, 4).await.unwrap(), b"0123");
    }
}
