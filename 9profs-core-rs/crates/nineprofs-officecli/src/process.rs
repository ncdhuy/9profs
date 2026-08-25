use std::{ffi::OsString, path::PathBuf, process::Stdio};

#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

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

#[cfg(windows)]
#[derive(Debug)]
struct ProcessTreeGuard {
    job: Option<OwnedHandle>,
}

#[cfg(windows)]
impl ProcessTreeGuard {
    fn new(process: std::os::windows::io::RawHandle) -> Result<Self, std::io::Error> {
        let job = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = unsafe { OwnedHandle::from_raw_handle(job) };

        let mut limits: JobObjectExtendedLimitInformation = unsafe { std::mem::zeroed() };
        limits.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &mut limits as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<JobObjectExtendedLimitInformation>() as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error());
        }

        let assigned = unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) };
        if assigned == 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(Self { job: Some(job) })
    }

    fn close(&mut self) {
        // Closing a job configured with KILL_ON_JOB_CLOSE terminates the
        // OfficeCLI process and any resident descendants it created.
        self.job.take();
    }
}

#[cfg(windows)]
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
#[cfg(windows)]
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

#[cfg(windows)]
#[repr(C)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[cfg(windows)]
#[repr(C)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[cfg(windows)]
#[repr(C)]
struct JobObjectExtendedLimitInformation {
    basic: JobObjectBasicLimitInformation,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(
        job_attributes: *mut std::ffi::c_void,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn SetInformationJobObject(
        job: *mut std::ffi::c_void,
        information_class: u32,
        information: *mut std::ffi::c_void,
        information_length: u32,
    ) -> i32;
    fn AssignProcessToJobObject(
        job: *mut std::ffi::c_void,
        process: std::os::windows::io::RawHandle,
    ) -> i32;
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
        #[cfg(windows)]
        let mut process_tree_guard =
            ProcessTreeGuard::new(child.raw_handle().ok_or(ProcessError::Reader)?)
                .map_err(ProcessError::Io)?;
        let stdout = child.stdout.take().ok_or(ProcessError::Reader)?;
        let stderr = child.stderr.take().ok_or(ProcessError::Reader)?;
        let stdout_task = tokio::spawn(read_limited(stdout, max_output_bytes));
        let stderr_task = tokio::spawn(read_limited(stderr, max_output_bytes));
        let status = child.wait().await.map_err(ProcessError::Io)?;
        let stdout = stdout_task.await.map_err(|_| ProcessError::Reader)??;
        let stderr = stderr_task.await.map_err(|_| ProcessError::Reader)??;
        #[cfg(windows)]
        process_tree_guard.close();
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
