use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ExitStatus, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::{PROTOCOL_VERSION, ProtocolMessage, Shutdown};

const DEFAULT_STDERR_TAIL_LINES: usize = 64;
const DEFAULT_STDOUT_QUEUE_CAPACITY: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterProcessConfig {
    executable: PathBuf,
    args: Vec<String>,
    current_dir: Option<PathBuf>,
    envs: Vec<(String, String)>,
    stderr_tail_lines: usize,
    stdout_queue_capacity: usize,
}

impl AdapterProcessConfig {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            current_dir: None,
            envs: Vec::new(),
            stderr_tail_lines: DEFAULT_STDERR_TAIL_LINES,
            stdout_queue_capacity: DEFAULT_STDOUT_QUEUE_CAPACITY,
        }
    }

    pub fn arg(mut self, value: impl Into<String>) -> Self {
        self.args.push(value.into());
        self
    }

    pub fn current_dir(mut self, value: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(value.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    pub fn stderr_tail_lines(mut self, lines: usize) -> Self {
        self.stderr_tail_lines = lines;
        self
    }

    /// Bounds decoded protocol ingress. A full queue applies pipe backpressure; it never drops
    /// response envelopes before the runtime can correlate them.
    pub fn stdout_queue_capacity(mut self, messages: usize) -> Self {
        self.stdout_queue_capacity = messages.max(1);
        self
    }
}

#[derive(Debug)]
pub struct AdapterProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_rx: mpsc::Receiver<Result<ProtocolMessage, ProcessError>>,
    stderr_tail: Arc<Mutex<BoundedText>>,
    last_status: Option<ExitStatus>,
}

impl AdapterProcess {
    pub fn start(config: AdapterProcessConfig) -> Result<Self, ProcessError> {
        let mut command = std::process::Command::new(&config.executable);
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(current_dir) = config.current_dir {
            command.current_dir(current_dir);
        }
        for (key, value) in config.envs {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(ProcessError::Spawn)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(ProcessError::MissingPipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(ProcessError::MissingPipe("stderr"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or(ProcessError::MissingPipe("stdin"))?;
        let (stdout_tx, stdout_rx) = mpsc::sync_channel(config.stdout_queue_capacity);
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let message = match line {
                    Ok(line) => serde_json::from_str::<ProtocolMessage>(&line)
                        .map_err(ProcessError::DecodeStdout),
                    Err(error) => Err(ProcessError::ReadStdout(error)),
                };
                if stdout_tx.send(message).is_err() {
                    break;
                }
            }
        });

        let stderr_tail = Arc::new(Mutex::new(BoundedText::new(config.stderr_tail_lines)));
        let stderr_tail_thread = Arc::clone(&stderr_tail);
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Ok(mut tail) = stderr_tail_thread.lock() {
                    tail.push(line);
                } else {
                    break;
                }
            }
        });

        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout_rx,
            stderr_tail,
            last_status: None,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn status(&mut self) -> ProcessStatus {
        if self.last_status.is_none() {
            self.last_status = self.child.try_wait().ok().flatten();
        }
        if let Some(status) = self.last_status {
            ProcessStatus::Exited {
                pid: self.child.id(),
                code: status.code(),
                success: status.success(),
            }
        } else {
            ProcessStatus::Running {
                pid: self.child.id(),
            }
        }
    }

    pub fn write_message(&mut self, message: &ProtocolMessage) -> Result<(), ProcessError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(ProcessError::StdinClosed);
        };
        serde_json::to_writer(&mut *stdin, message).map_err(ProcessError::EncodeStdin)?;
        stdin.write_all(b"\n").map_err(ProcessError::WriteStdin)?;
        stdin.flush().map_err(ProcessError::WriteStdin)
    }

    pub fn read_stdout_message(
        &mut self,
        timeout: Duration,
    ) -> Result<ProtocolMessage, ProcessError> {
        match self.stdout_rx.recv_timeout(timeout) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ProcessError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ProcessError::StdoutClosed),
        }
    }

    pub fn try_read_stdout_message(&mut self) -> Result<Option<ProtocolMessage>, ProcessError> {
        match self.stdout_rx.try_recv() {
            Ok(message) => message.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(ProcessError::StdoutClosed),
        }
    }

    pub fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .map(|tail| tail.joined())
            .unwrap_or_default()
    }

    pub fn stderr_dropped_lines(&self) -> usize {
        self.stderr_tail
            .lock()
            .map(|tail| tail.dropped())
            .unwrap_or_default()
    }

    pub fn stop(
        &mut self,
        graceful_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<ExitStatus, ProcessError> {
        if let Some(status) = self.last_status {
            return Ok(status);
        }

        let _ = self.write_message(&ProtocolMessage::Shutdown(Shutdown {
            protocol: PROTOCOL_VERSION,
        }));
        drop(self.stdin.take());

        let graceful_deadline = Instant::now() + graceful_timeout;
        while Instant::now() < graceful_deadline {
            if let Some(status) = self.child.try_wait().map_err(ProcessError::Wait)? {
                self.last_status = Some(status);
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(10));
        }

        self.child.kill().map_err(ProcessError::Kill)?;
        let kill_deadline = Instant::now() + kill_timeout;
        while Instant::now() < kill_deadline {
            if let Some(status) = self.child.try_wait().map_err(ProcessError::Wait)? {
                self.last_status = Some(status);
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(10));
        }
        let status = self.child.wait().map_err(ProcessError::Wait)?;
        self.last_status = Some(status);
        Ok(status)
    }
}

impl Drop for AdapterProcess {
    fn drop(&mut self) {
        if self.last_status.is_none() {
            let _ = self.stop(Duration::from_millis(50), Duration::from_millis(50));
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessStatus {
    Running {
        pid: u32,
    },
    Exited {
        pid: u32,
        code: Option<i32>,
        success: bool,
    },
}

#[derive(Debug)]
pub enum ProcessError {
    Spawn(io::Error),
    MissingPipe(&'static str),
    EncodeStdin(serde_json::Error),
    WriteStdin(io::Error),
    StdinClosed,
    ReadStdout(io::Error),
    DecodeStdout(serde_json::Error),
    StdoutClosed,
    Timeout,
    Wait(io::Error),
    Kill(io::Error),
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "failed to start adapter process: {error}"),
            Self::MissingPipe(pipe) => write!(formatter, "adapter process missing {pipe} pipe"),
            Self::EncodeStdin(error) => {
                write!(formatter, "failed to encode stdin message: {error}")
            }
            Self::WriteStdin(error) => write!(formatter, "failed to write adapter stdin: {error}"),
            Self::StdinClosed => write!(formatter, "adapter stdin is closed"),
            Self::ReadStdout(error) => write!(formatter, "failed to read adapter stdout: {error}"),
            Self::DecodeStdout(error) => {
                write!(
                    formatter,
                    "failed to decode adapter stdout protocol: {error}"
                )
            }
            Self::StdoutClosed => write!(formatter, "adapter stdout closed"),
            Self::Timeout => write!(formatter, "timed out waiting for adapter stdout"),
            Self::Wait(error) => write!(formatter, "failed to wait for adapter process: {error}"),
            Self::Kill(error) => write!(formatter, "failed to kill adapter process: {error}"),
        }
    }
}

impl Error for ProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn(error)
            | Self::WriteStdin(error)
            | Self::ReadStdout(error)
            | Self::Wait(error)
            | Self::Kill(error) => Some(error),
            Self::EncodeStdin(error) | Self::DecodeStdout(error) => Some(error),
            Self::MissingPipe(_) | Self::StdinClosed | Self::StdoutClosed | Self::Timeout => None,
        }
    }
}

#[derive(Debug)]
struct BoundedText {
    lines: VecDeque<String>,
    capacity: usize,
    dropped: usize,
}

impl BoundedText {
    fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity),
            capacity,
            dropped: 0,
        }
    }

    fn push(&mut self, line: String) {
        if self.capacity == 0 {
            self.dropped += 1;
            return;
        }
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
        self.lines.push_back(line);
    }

    fn joined(&self) -> String {
        self.lines.iter().cloned().collect::<Vec<_>>().join("\n")
    }

    fn dropped(&self) -> usize {
        self.dropped
    }
}
