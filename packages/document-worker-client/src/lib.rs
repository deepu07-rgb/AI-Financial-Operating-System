//! Bounded, versioned process boundary for the local Python document worker.

use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use aifos_domain::{Document, EntityId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum WorkerClientError {
    #[error("worker configuration is invalid: {0}")]
    InvalidConfiguration(&'static str),
    #[error("worker process could not be started or controlled: {0}")]
    Process(#[source] std::io::Error),
    #[error("worker exceeded its {0:?} timeout")]
    Timeout(Duration),
    #[error("worker output exceeded the permitted size")]
    OutputTooLarge,
    #[error("worker was cancelled at a safe process boundary")]
    Cancelled,
    #[error("worker response is not valid UTF-8 JSON: {0}")]
    InvalidResponse(String),
    #[error("worker protocol invariant failed: {0}")]
    Protocol(&'static str),
    #[error("worker rejected the job ({code}): {message}")]
    Rejected { code: String, message: String },
}

pub type Result<T> = std::result::Result<T, WorkerClientError>;

#[derive(Debug, Clone)]
pub struct WorkerCommand {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub timeout: Duration,
    pub maximum_output_bytes: usize,
}

impl WorkerCommand {
    #[must_use]
    pub fn python(
        python: impl AsRef<Path>,
        service_root: impl AsRef<Path>,
        allowed_root: impl AsRef<Path>,
        work_root: impl AsRef<Path>,
        tesseract: impl AsRef<Path>,
    ) -> Self {
        Self {
            program: python.as_ref().to_path_buf(),
            arguments: vec![
                OsString::from("-I"),
                service_root
                    .as_ref()
                    .join("worker_entry.py")
                    .into_os_string(),
                OsString::from("--allowed-root"),
                allowed_root.as_ref().as_os_str().to_owned(),
                OsString::from("--work-root"),
                work_root.as_ref().as_os_str().to_owned(),
                OsString::from("--tesseract"),
                tesseract.as_ref().as_os_str().to_owned(),
            ],
            timeout: DEFAULT_TIMEOUT,
            maximum_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkerRequest<'a> {
    protocol_version: u16,
    job_id: String,
    operation: &'static str,
    document_token: String,
    source_path: &'a Path,
    expected_sha256: &'a str,
    media_type: &'static str,
    supplier_templates: &'a [SupplierTemplateInstruction],
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupplierTemplateInstruction {
    pub supplier_key: String,
    pub field_name: String,
    pub label_text: String,
    pub version_id: String,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct PreviewRequest<'a> {
    protocol_version: u16,
    job_id: String,
    operation: &'static str,
    document_token: String,
    source_path: &'a Path,
    expected_sha256: &'a str,
    media_type: &'static str,
    page_index: u32,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ReportRequest<'a> {
    protocol_version: u16,
    job_id: String,
    operation: &'static str,
    report_token: String,
    contract_path: &'a Path,
    expected_sha256: &'a str,
    output_path: &'a Path,
    format: &'static str,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerDocument {
    pub token: String,
    pub sha256: String,
    pub media_type: String,
    pub page_count: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerEngine {
    pub name: String,
    pub version: String,
    pub language: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerTiming {
    pub total_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerProbe {
    pub protocol_version: u16,
    pub worker: String,
    pub python: String,
    pub ready: bool,
    pub engine: Option<WorkerEngine>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewSuccess {
    pub protocol_version: u16,
    pub job_id: String,
    pub document_token: String,
    pub sha256: String,
    pub page_index: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub data_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRenderSuccess {
    pub protocol_version: u16,
    pub job_id: String,
    pub report_token: String,
    pub contract_sha256: String,
    pub output_sha256: String,
    pub byte_size: u64,
    pub page_count: u32,
    pub format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSuccess {
    pub protocol_version: u16,
    pub job_id: String,
    pub document: WorkerDocument,
    pub engine: WorkerEngine,
    pub preprocessing: Value,
    pub pages: Vec<Value>,
    pub fields: Value,
    pub line_items: Vec<Value>,
    pub validation: Value,
    pub warnings: Vec<Value>,
    pub timing: WorkerTiming,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerFailure {
    protocol_version: u16,
    job_id: Option<String>,
    error: WorkerErrorBody,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerErrorBody {
    code: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum WorkerResponse {
    Succeeded(Box<WorkerSuccess>),
    Failed(WorkerFailure),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PreviewResponse {
    Succeeded(Box<PreviewSuccess>),
    Failed(WorkerFailure),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum ReportResponse {
    Succeeded(Box<ReportRenderSuccess>),
    Failed(WorkerFailure),
}

pub fn probe_python_worker(
    python: impl AsRef<Path>,
    service_root: impl AsRef<Path>,
    tesseract: impl AsRef<Path>,
) -> Result<WorkerProbe> {
    let command = WorkerCommand {
        program: python.as_ref().to_path_buf(),
        arguments: vec![
            OsString::from("-I"),
            service_root
                .as_ref()
                .join("worker_entry.py")
                .into_os_string(),
            OsString::from("--probe"),
            OsString::from("--tesseract"),
            tesseract.as_ref().as_os_str().to_owned(),
        ],
        timeout: Duration::from_secs(10),
        maximum_output_bytes: 64 * 1024,
    };
    if !command.program.is_file() {
        return Err(WorkerClientError::InvalidConfiguration(
            "worker executable does not exist",
        ));
    }
    let (status, stdout, stderr, output_too_large) = run(&command, b"")?;
    if output_too_large {
        return Err(WorkerClientError::OutputTooLarge);
    }
    if !status.success() {
        return Err(WorkerClientError::InvalidResponse(format!(
            "probe exited with {status}: {}",
            String::from_utf8_lossy(&stderr)
        )));
    }
    let probe: WorkerProbe = serde_json::from_slice(&stdout)
        .map_err(|error| WorkerClientError::InvalidResponse(error.to_string()))?;
    if probe.protocol_version != PROTOCOL_VERSION || probe.worker != "aifos-document-worker" {
        return Err(WorkerClientError::Protocol(
            "probe response does not identify this worker protocol",
        ));
    }
    if probe.ready != probe.engine.is_some() {
        return Err(WorkerClientError::Protocol(
            "probe readiness and engine identity disagree",
        ));
    }
    Ok(probe)
}

pub fn process_document(
    command: &WorkerCommand,
    document: &Document,
    source_path: &Path,
) -> Result<WorkerSuccess> {
    if !command.program.is_file() {
        return Err(WorkerClientError::InvalidConfiguration(
            "worker executable does not exist",
        ));
    }
    if command.maximum_output_bytes == 0 {
        return Err(WorkerClientError::InvalidConfiguration(
            "maximum output must be positive",
        ));
    }
    let job_id = EntityId::new().to_string();
    let document_token = EntityId::new().to_string();
    let request = WorkerRequest {
        protocol_version: PROTOCOL_VERSION,
        job_id: job_id.clone(),
        operation: "process_document",
        document_token: document_token.clone(),
        source_path,
        expected_sha256: &document.sha256,
        media_type: document.media_type.as_mime_type(),
        supplier_templates: &[],
    };
    let input = serde_json::to_vec(&request)
        .map_err(|error| WorkerClientError::InvalidResponse(error.to_string()))?;
    let (status, stdout, stderr, output_too_large) = run(command, &input)?;
    if output_too_large {
        return Err(WorkerClientError::OutputTooLarge);
    }
    let response: WorkerResponse = serde_json::from_slice(&stdout).map_err(|error| {
        WorkerClientError::InvalidResponse(format!(
            "{error}; process status {status}; stderr {}",
            String::from_utf8_lossy(&stderr)
        ))
    })?;
    match response {
        WorkerResponse::Succeeded(success) => {
            if !status.success()
                || success.protocol_version != PROTOCOL_VERSION
                || success.job_id != job_id
                || success.document.token != document_token
                || success.document.sha256 != document.sha256
                || success.document.media_type != document.media_type.as_mime_type()
            {
                return Err(WorkerClientError::Protocol(
                    "success response does not match its request",
                ));
            }
            Ok(*success)
        }
        WorkerResponse::Failed(failure) => {
            if failure.protocol_version != PROTOCOL_VERSION
                || failure.job_id.as_deref() != Some(job_id.as_str())
            {
                return Err(WorkerClientError::Protocol(
                    "failure response does not match its request",
                ));
            }
            Err(WorkerClientError::Rejected {
                code: failure.error.code,
                message: failure.error.message,
            })
        }
    }
}

pub fn process_document_with_cancel(
    command: &WorkerCommand,
    document: &Document,
    source_path: &Path,
    cancel: &AtomicBool,
) -> Result<WorkerSuccess> {
    process_document_with_templates_and_cancel(command, document, source_path, &[], cancel)
}

pub fn process_document_with_templates_and_cancel(
    command: &WorkerCommand,
    document: &Document,
    source_path: &Path,
    supplier_templates: &[SupplierTemplateInstruction],
    cancel: &AtomicBool,
) -> Result<WorkerSuccess> {
    if !command.program.is_file() {
        return Err(WorkerClientError::InvalidConfiguration(
            "worker executable does not exist",
        ));
    }
    let job_id = EntityId::new().to_string();
    let document_token = EntityId::new().to_string();
    let request = WorkerRequest {
        protocol_version: PROTOCOL_VERSION,
        job_id: job_id.clone(),
        operation: "process_document",
        document_token: document_token.clone(),
        source_path,
        expected_sha256: &document.sha256,
        media_type: document.media_type.as_mime_type(),
        supplier_templates,
    };
    let input = serde_json::to_vec(&request)
        .map_err(|error| WorkerClientError::InvalidResponse(error.to_string()))?;
    let (status, stdout, stderr, output_too_large) = run_inner(command, &input, Some(cancel))?;
    if output_too_large {
        return Err(WorkerClientError::OutputTooLarge);
    }
    let response: WorkerResponse = serde_json::from_slice(&stdout).map_err(|error| {
        WorkerClientError::InvalidResponse(format!(
            "{error}; process status {status}; stderr {}",
            String::from_utf8_lossy(&stderr)
        ))
    })?;
    match response {
        WorkerResponse::Succeeded(success)
            if status.success()
                && success.protocol_version == PROTOCOL_VERSION
                && success.job_id == job_id
                && success.document.token == document_token
                && success.document.sha256 == document.sha256
                && success.document.media_type == document.media_type.as_mime_type() =>
        {
            Ok(*success)
        }
        WorkerResponse::Succeeded(_) => Err(WorkerClientError::Protocol(
            "success response does not match its request",
        )),
        WorkerResponse::Failed(failure)
            if failure.protocol_version == PROTOCOL_VERSION
                && failure.job_id.as_deref() == Some(job_id.as_str()) =>
        {
            Err(WorkerClientError::Rejected {
                code: failure.error.code,
                message: failure.error.message,
            })
        }
        WorkerResponse::Failed(_) => Err(WorkerClientError::Protocol(
            "failure response does not match its request",
        )),
    }
}

pub fn render_document_preview(
    python: impl AsRef<Path>,
    service_root: impl AsRef<Path>,
    allowed_root: impl AsRef<Path>,
    document: &Document,
    source_path: &Path,
    page_index: u32,
) -> Result<PreviewSuccess> {
    let command = WorkerCommand {
        program: python.as_ref().to_path_buf(),
        arguments: vec![
            OsString::from("-I"),
            service_root
                .as_ref()
                .join("worker_entry.py")
                .into_os_string(),
            OsString::from("--allowed-root"),
            allowed_root.as_ref().as_os_str().to_owned(),
        ],
        timeout: Duration::from_secs(30),
        maximum_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
    };
    if !command.program.is_file() {
        return Err(WorkerClientError::InvalidConfiguration(
            "worker executable does not exist",
        ));
    }
    let job_id = EntityId::new().to_string();
    let document_token = EntityId::new().to_string();
    let request = PreviewRequest {
        protocol_version: PROTOCOL_VERSION,
        job_id: job_id.clone(),
        operation: "render_preview",
        document_token: document_token.clone(),
        source_path,
        expected_sha256: &document.sha256,
        media_type: document.media_type.as_mime_type(),
        page_index,
    };
    let input = serde_json::to_vec(&request)
        .map_err(|error| WorkerClientError::InvalidResponse(error.to_string()))?;
    let (status, stdout, stderr, output_too_large) = run(&command, &input)?;
    if output_too_large {
        return Err(WorkerClientError::OutputTooLarge);
    }
    let response: PreviewResponse = serde_json::from_slice(&stdout).map_err(|error| {
        WorkerClientError::InvalidResponse(format!(
            "{error}; process status {status}; stderr {}",
            String::from_utf8_lossy(&stderr)
        ))
    })?;
    match response {
        PreviewResponse::Succeeded(preview) => {
            if !status.success()
                || preview.protocol_version != PROTOCOL_VERSION
                || preview.job_id != job_id
                || preview.document_token != document_token
                || preview.sha256 != document.sha256
                || preview.page_index != page_index
                || preview.source_width == 0
                || preview.source_height == 0
                || !preview.data_url.starts_with("data:image/jpeg;base64,")
            {
                return Err(WorkerClientError::Protocol(
                    "preview response does not match its request",
                ));
            }
            Ok(*preview)
        }
        PreviewResponse::Failed(failure) => {
            if failure.protocol_version != PROTOCOL_VERSION
                || failure.job_id.as_deref() != Some(job_id.as_str())
            {
                return Err(WorkerClientError::Protocol(
                    "preview failure response does not match its request",
                ));
            }
            Err(WorkerClientError::Rejected {
                code: failure.error.code,
                message: failure.error.message,
            })
        }
    }
}

pub fn render_report_pdf(
    python: impl AsRef<Path>,
    service_root: impl AsRef<Path>,
    allowed_root: impl AsRef<Path>,
    work_root: impl AsRef<Path>,
    contract_path: &Path,
    expected_sha256: &str,
    output_path: &Path,
) -> Result<ReportRenderSuccess> {
    let command = WorkerCommand {
        program: python.as_ref().to_path_buf(),
        arguments: vec![
            OsString::from("-I"),
            service_root
                .as_ref()
                .join("worker_entry.py")
                .into_os_string(),
            OsString::from("--allowed-root"),
            allowed_root.as_ref().as_os_str().to_owned(),
            OsString::from("--work-root"),
            work_root.as_ref().as_os_str().to_owned(),
        ],
        timeout: Duration::from_secs(60),
        maximum_output_bytes: 128 * 1024,
    };
    if !command.program.is_file() {
        return Err(WorkerClientError::InvalidConfiguration(
            "worker executable does not exist",
        ));
    }
    let job_id = EntityId::new().to_string();
    let report_token = EntityId::new().to_string();
    let request = ReportRequest {
        protocol_version: PROTOCOL_VERSION,
        job_id: job_id.clone(),
        operation: "render_report",
        report_token: report_token.clone(),
        contract_path,
        expected_sha256,
        output_path,
        format: "pdf",
    };
    let input = serde_json::to_vec(&request)
        .map_err(|error| WorkerClientError::InvalidResponse(error.to_string()))?;
    let (status, stdout, stderr, output_too_large) = run(&command, &input)?;
    if output_too_large {
        return Err(WorkerClientError::OutputTooLarge);
    }
    let response: ReportResponse = serde_json::from_slice(&stdout).map_err(|error| {
        WorkerClientError::InvalidResponse(format!(
            "{error}; process status {status}; stderr {}",
            String::from_utf8_lossy(&stderr)
        ))
    })?;
    match response {
        ReportResponse::Succeeded(success) => {
            if !status.success()
                || success.protocol_version != PROTOCOL_VERSION
                || success.job_id != job_id
                || success.report_token != report_token
                || success.contract_sha256 != expected_sha256
                || success.format != "pdf"
                || success.byte_size == 0
                || success.page_count == 0
                || success.output_sha256.len() != 64
            {
                return Err(WorkerClientError::Protocol(
                    "report response does not match its request",
                ));
            }
            Ok(*success)
        }
        ReportResponse::Failed(failure) => {
            if failure.protocol_version != PROTOCOL_VERSION
                || failure.job_id.as_deref() != Some(job_id.as_str())
            {
                return Err(WorkerClientError::Protocol(
                    "report failure response does not match its request",
                ));
            }
            Err(WorkerClientError::Rejected {
                code: failure.error.code,
                message: failure.error.message,
            })
        }
    }
}

fn run(
    command: &WorkerCommand,
    input: &[u8],
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>, bool)> {
    run_inner(command, input, None)
}

fn run_inner(
    command: &WorkerCommand,
    input: &[u8],
    cancel: Option<&AtomicBool>,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>, bool)> {
    let mut child = Command::new(&command.program)
        .args(&command.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONHASHSEED", "0")
        .spawn()
        .map_err(WorkerClientError::Process)?;
    #[cfg(windows)]
    let _job = WindowsJob::assign(&child)?;
    child
        .stdin
        .take()
        .ok_or(WorkerClientError::Protocol("worker stdin is unavailable"))?
        .write_all(input)
        .map_err(WorkerClientError::Process)?;

    let stdout = child
        .stdout
        .take()
        .ok_or(WorkerClientError::Protocol("worker stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(WorkerClientError::Protocol("worker stderr is unavailable"))?;
    let stdout_result = Arc::new(Mutex::new(None));
    let stderr_result = Arc::new(Mutex::new(None));
    let stdout_thread = read_bounded(stdout, command.maximum_output_bytes, stdout_result.clone());
    let stderr_thread = read_bounded(stderr, 16 * 1024, stderr_result.clone());

    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(WorkerClientError::Process)? {
            Some(status) => break status,
            None if started.elapsed() >= command.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                stdout_thread.join().ok();
                stderr_thread.join().ok();
                return Err(WorkerClientError::Timeout(command.timeout));
            }
            None if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) => {
                let _ = child.kill();
                let _ = child.wait();
                stdout_thread.join().ok();
                stderr_thread.join().ok();
                return Err(WorkerClientError::Cancelled);
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    };
    stdout_thread
        .join()
        .map_err(|_| WorkerClientError::Protocol("stdout reader panicked"))?;
    stderr_thread
        .join()
        .map_err(|_| WorkerClientError::Protocol("stderr reader panicked"))?;
    let (stdout, stdout_exceeded) = stdout_result
        .lock()
        .map_err(|_| WorkerClientError::Protocol("stdout result lock poisoned"))?
        .take()
        .ok_or(WorkerClientError::Protocol(
            "stdout reader produced no result",
        ))?
        .map_err(WorkerClientError::Process)?;
    let (stderr, stderr_exceeded) = stderr_result
        .lock()
        .map_err(|_| WorkerClientError::Protocol("stderr result lock poisoned"))?
        .take()
        .ok_or(WorkerClientError::Protocol(
            "stderr reader produced no result",
        ))?
        .map_err(WorkerClientError::Process)?;
    Ok((status, stdout, stderr, stdout_exceeded || stderr_exceeded))
}

#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl WindowsJob {
    fn assign(child: &std::process::Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::GetLastError,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOB_OBJECT_LIMIT_PROCESS_MEMORY,
            },
        };
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(WorkerClientError::Process(
                std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32),
            ));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        limits.BasicLimitInformation.ActiveProcessLimit = 2;
        limits.ProcessMemoryLimit = 768 * 1024 * 1024;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        let assigned = configured != 0
            && unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as _) } != 0;
        if !assigned {
            let error = std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32);
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(WorkerClientError::Process(error));
        }
        Ok(Self(job))
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

type ReadResult = std::io::Result<(Vec<u8>, bool)>;

fn read_bounded<R: Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    result: Arc<Mutex<Option<ReadResult>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut collected = Vec::with_capacity(limit.min(64 * 1024));
        let mut exceeded = false;
        let mut buffer = [0_u8; 8192];
        let read_result = loop {
            match reader.read(&mut buffer) {
                Ok(0) => break Ok((collected, exceeded)),
                Ok(read) => {
                    let remaining = limit.saturating_sub(collected.len());
                    collected.extend_from_slice(&buffer[..read.min(remaining)]);
                    exceeded |= read > remaining;
                }
                Err(error) => break Err(error),
            }
        };
        if let Ok(mut guard) = result.lock() {
            *guard = Some(read_result);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_marks_excess_and_drains_the_stream() {
        let result = Arc::new(Mutex::new(None));
        let thread = read_bounded(std::io::Cursor::new(vec![7_u8; 100]), 16, result.clone());
        thread.join().unwrap();
        let guard = result.lock().unwrap();
        let (bytes, exceeded) = guard.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(bytes.len(), 16);
        assert!(*exceeded);
    }

    #[test]
    fn response_contract_rejects_unknown_success_fields() {
        let raw = br#"{"status":"succeeded","protocol_version":1,"job_id":"x","document":{"token":"x","sha256":"x","media_type":"image/png","page_count":1},"engine":{"name":"x","version":"x","language":"eng"},"preprocessing":{},"pages":[],"fields":{},"line_items":[],"validation":{},"warnings":[],"timing":{"total_ms":1},"unexpected":true}"#;
        assert!(serde_json::from_slice::<WorkerResponse>(raw).is_err());
    }

    #[test]
    fn report_response_contract_rejects_unknown_success_fields() {
        let raw = br#"{"status":"succeeded","protocol_version":1,"job_id":"x","report_token":"x","contract_sha256":"x","output_sha256":"x","byte_size":1,"page_count":1,"format":"pdf","unexpected":true}"#;
        assert!(serde_json::from_slice::<ReportResponse>(raw).is_err());
    }

    #[test]
    fn process_boundary_enforces_timeout() {
        #[cfg(windows)]
        let command = WorkerCommand {
            program: PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            arguments: vec![
                OsString::from("-NoProfile"),
                OsString::from("-Command"),
                OsString::from("Start-Sleep -Milliseconds 500"),
            ],
            timeout: Duration::from_millis(20),
            maximum_output_bytes: 1024,
        };
        #[cfg(unix)]
        let command = WorkerCommand {
            program: PathBuf::from("/bin/sh"),
            arguments: vec![OsString::from("-c"), OsString::from("sleep 0.5")],
            timeout: Duration::from_millis(20),
            maximum_output_bytes: 1024,
        };
        assert!(matches!(
            run(&command, b"{}"),
            Err(WorkerClientError::Timeout(_))
        ));
    }

    #[test]
    fn process_boundary_honors_cancellation() {
        #[cfg(windows)]
        let command = WorkerCommand {
            program: PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            arguments: vec![
                OsString::from("-NoProfile"),
                OsString::from("-Command"),
                OsString::from("Start-Sleep -Seconds 5"),
            ],
            timeout: Duration::from_secs(10),
            maximum_output_bytes: 1024,
        };
        #[cfg(unix)]
        let command = WorkerCommand {
            program: PathBuf::from("/bin/sh"),
            arguments: vec![OsString::from("-c"), OsString::from("sleep 5")],
            timeout: Duration::from_secs(10),
            maximum_output_bytes: 1024,
        };
        let cancel = AtomicBool::new(true);
        let started = Instant::now();
        assert!(matches!(
            run_inner(&command, b"{}", Some(&cancel)),
            Err(WorkerClientError::Cancelled)
        ));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
