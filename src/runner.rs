use crate::{DispatcherError, Formatter, WatcherParam};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

/// Child process controller
pub struct Runner {
    pub proc: Child,
    pub info: ProcInfo,
    pub output: Arc<Mutex<OutputBuffer>>,
}

/// Process information
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ProcInfo {
    pub job_id: u32,
    pub pid: u32,
    pub cmd_args: Vec<String>,
    pub state: ProcStatus,
    pub start: DateTime<Local>,
    pub end: Option<DateTime<Local>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ProcStatus {
    Spawned,
    Running,
    ExitOk,
    ExitErr(i32),
    Unknown(String),
}

impl ProcStatus {
    fn exited(&self) -> bool {
        matches!(self, ProcStatus::ExitOk | ProcStatus::ExitErr(_))
    }
}

/// Log line from captured stdout/stderr output
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LogLine {
    pub ts: DateTime<Local>,
    pub pid: u32,
    pub line: String,
    pub is_stderr: bool,
}

impl LogLine {
    pub fn log(&self, formatter: &Formatter) {
        let dt = self.ts.format("%F %T%.3f");
        let pid = self.pid;
        let line = &self.line;
        let color = formatter.log_color_proc(pid as usize, self.is_stderr);
        println!("{color}{dt} [{pid}] {line}{color:#}");
    }
}

/// Buffer for captured stdout/stderr output
pub struct OutputBuffer {
    lines: VecDeque<LogLine>,
    max_len: Option<usize>,
}

impl OutputBuffer {
    pub fn new(max_len: Option<usize>) -> Self {
        OutputBuffer {
            max_len,
            lines: VecDeque::new(),
        }
    }
    pub fn push(&mut self, line: LogLine) {
        self.lines.push_back(line);
        if let Some(max_len) = self.max_len {
            if self.lines.len() > max_len {
                let _ = self.lines.pop_front();
            }
        }
    }
    pub fn lines_since(&self, last_seen: &mut DateTime<Local>) -> impl Iterator<Item = &LogLine> {
        let ts = *last_seen;
        if let Some(entry) = self.lines.back() {
            *last_seen = entry.ts;
        }
        self.lines.iter().skip_while(move |entry| ts >= entry.ts)
    }
}

impl Runner {
    pub fn spawn(
        job_id: u32,
        args: &[String],
        channel: mpsc::Sender<WatcherParam>,
    ) -> Result<Self, DispatcherError> {
        let cmd_args = args.to_vec();
        let mut cmd = VecDeque::from(args.to_owned());
        let Some(exe) = cmd.pop_front() else {
            return Err(DispatcherError::EmptyProcCommandError);
        };
        // info!("Spawning {exe} {cmd:?}");

        let mut child = Command::new(exe)
            .args(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(DispatcherError::ProcSpawnError)?;
        let pid = child.id();

        // output listeners
        let max_len = 200; // TODO: Make configurable
        let output = Arc::new(Mutex::new(OutputBuffer::new(Some(max_len))));

        let buffer = output.clone();
        let stdout = child.stdout.take().unwrap();
        let _stdout_handle = thread::spawn(move || {
            output_listener(BufReader::new(stdout), pid, false, buffer, Some(channel))
        });

        let buffer = output.clone();
        let stderr = child.stderr.take().unwrap();
        let _stderr_handle =
            thread::spawn(move || output_listener(BufReader::new(stderr), pid, true, buffer, None));

        let info = ProcInfo {
            job_id,
            pid,
            cmd_args,
            state: ProcStatus::Spawned,
            start: Local::now(),
            end: None,
        };

        let child_proc = Runner {
            proc: child,
            info,
            output,
        };
        Ok(child_proc)
    }
    pub fn update_proc_info(&mut self) -> &ProcInfo {
        if self.info.end.is_none() {
            self.info.state = match self.proc.try_wait() {
                Ok(Some(status)) if status.success() => ProcStatus::ExitOk,
                Ok(Some(status)) => ProcStatus::ExitErr(status.code().unwrap_or(0)),
                Ok(None) => ProcStatus::Running,
                Err(e) => ProcStatus::Unknown(e.to_string()),
            };
        }
        &self.info
    }
    pub fn is_running(&mut self) -> bool {
        !self.update_proc_info().state.exited()
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.proc.kill().unwrap();
    }
}

fn output_listener<R: Read>(
    reader: BufReader<R>,
    pid: u32,
    is_stderr: bool,
    buffer: Arc<Mutex<OutputBuffer>>,
    channel: Option<mpsc::Sender<WatcherParam>>,
) {
    reader.lines().map_while(Result::ok).for_each(|line| {
        let ts = Local::now();
        if is_stderr {
            eprintln!("[{pid}] {line}");
        } else {
            println!("[{pid}] {line}");
        }
        if let Ok(mut buffer) = buffer.lock() {
            let entry = LogLine {
                ts,
                pid,
                is_stderr,
                line,
            };
            buffer.push(entry);
        }
    });
    if let Some(channel) = channel {
        let ts = Local::now();
        if let Ok(mut buffer) = buffer.lock() {
            let entry = LogLine {
                ts,
                pid,
                is_stderr,
                line: "<process terminated>".to_string(),
            };
            buffer.push(entry);
        }
        // Notify watcher
        channel.send(pid).unwrap();
    }
}
