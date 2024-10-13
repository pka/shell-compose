use crate::{DispatcherError, Formatter, JobId, Pid, RestartInfo};
use chrono::{DateTime, Local};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{self, Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind, Users};

/// Child process controller
pub struct Runner {
    pub proc: Child,
    pub info: ProcInfo,
    pub restart_info: RestartInfo,
    /// Flag set in stop/down command to prevent restart
    pub user_terminated: bool,
    pub output: Arc<Mutex<OutputBuffer>>,
}

/// Process information
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ProcInfo {
    pub job_id: JobId,
    pub pid: Pid,
    pub cmd_args: Vec<String>,
    pub state: ProcStatus,
    pub start: DateTime<Local>,
    pub end: Option<DateTime<Local>>,
    /// Total CPU usage (in %)
    /// See <https://docs.rs/sysinfo/latest/i686-pc-windows-msvc/sysinfo/struct.Process.html#method.cpu_usage>
    pub cpu: f32,
    /// Memory usage (in bytes).
    /// See <https://docs.rs/sysinfo/latest/i686-pc-windows-msvc/sysinfo/struct.Process.html#method.memory>
    pub memory: u64,
    /// Virtual memory usage (in bytes).
    /// <https://docs.rs/sysinfo/latest/i686-pc-windows-msvc/sysinfo/struct.Process.html#method.virtual_memory>
    pub virtual_memory: u64,
    /// Total number of written bytes.
    /// <https://docs.rs/sysinfo/latest/i686-pc-windows-msvc/sysinfo/struct.Process.html#method.disk_usage>
    pub total_written_bytes: u64,
    /// Written bytes per second.
    pub written_bytes: u64,
    /// Total number of read bytes.
    pub total_read_bytes: u64,
    /// Read bytes per second.
    pub read_bytes: u64,
}

impl ProcInfo {
    pub fn program(&self) -> &str {
        self.cmd_args.first().map(|s| s.as_str()).unwrap_or("")
    }
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
    pub fn exited(&self) -> bool {
        matches!(self, ProcStatus::ExitOk | ProcStatus::ExitErr(_))
    }
}

/// Log line from captured stdout/stderr output
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LogLine {
    pub ts: DateTime<Local>,
    pub job_id: JobId,
    pub pid: Pid,
    pub line: String,
    pub is_stderr: bool,
}

impl LogLine {
    pub fn log(&self, formatter: &Formatter) {
        let dt = self.ts.format("%F %T%.3f");
        let job_id = self.job_id;
        let pid = self.pid;
        let line = &self.line;
        let color = formatter.log_color_proc(job_id as usize, self.is_stderr);
        println!("{color}{dt} [{job_id}|{pid}] {line}{color:#}");
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
        job_id: JobId,
        args: &[String],
        restart_info: RestartInfo,
        channel: mpsc::Sender<Pid>,
    ) -> Result<Self, DispatcherError> {
        let cmd_args = args.to_vec();
        let mut cmd = VecDeque::from(args.to_owned());
        let Some(exe) = cmd.pop_front() else {
            return Err(DispatcherError::EmptyProcCommandError);
        };
        // info!("Spawning {exe} {cmd:?}");

        let mut child = Command::new(exe)
            .args(cmd)
            .stdin(Stdio::piped())
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
            output_listener(
                BufReader::new(stdout),
                job_id,
                pid,
                false,
                buffer,
                Some(channel),
            )
        });

        let buffer = output.clone();
        let stderr = child.stderr.take().unwrap();
        let _stderr_handle = thread::spawn(move || {
            output_listener(BufReader::new(stderr), job_id, pid, true, buffer, None)
        });

        let info = ProcInfo {
            job_id,
            pid,
            cmd_args,
            state: ProcStatus::Spawned,
            start: Local::now(),
            end: None,
            cpu: 0.0,
            memory: 0,
            virtual_memory: 0,
            total_written_bytes: 0,
            written_bytes: 0,
            total_read_bytes: 0,
            read_bytes: 0,
        };

        let child_proc = Runner {
            proc: child,
            info,
            restart_info,
            user_terminated: false,
            output,
        };
        Ok(child_proc)
    }
    pub fn update_proc_state(&mut self) -> &ProcInfo {
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
        !self.update_proc_state().state.exited()
    }
    pub fn terminate(&mut self) -> Result<(), std::io::Error> {
        if self.info.program() == "just" {
            // just does not propagate signals, so we have to kill its child process
            let just_pid = self.proc.id() as usize;
            let system = System::new_with_specifics(
                RefreshKind::new().with_processes(ProcessRefreshKind::new()),
            );
            if let Some((pid, process)) = system.processes().iter().find(|(_pid, process)| {
                process.parent().unwrap_or(0.into()) == just_pid.into()
                    && process.name() != "ctrl-c"
            }) {
                info!("Terminating process {pid} (parent process {just_pid})");
                process.kill(); // process.kill_with(Signal::Term)
            }
            // In an interactive terminal session, sending Ctrl-C terminates the running process.
            // let mut stdin = self.proc.stdin.take().unwrap();
            // stdin.write_all(&[3])?;
        } else {
            info!("Terminating process {}", self.proc.id());
            self.proc.kill()?;
        }
        Ok(())
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.terminate().ok();
    }
}

fn output_listener<R: Read>(
    reader: BufReader<R>,
    job_id: JobId,
    pid: Pid,
    is_stderr: bool,
    buffer: Arc<Mutex<OutputBuffer>>,
    channel: Option<mpsc::Sender<Pid>>,
) {
    reader.lines().map_while(Result::ok).for_each(|line| {
        let ts = Local::now();
        if is_stderr {
            eprintln!("[{job_id}|{pid}] {line}");
        } else {
            println!("[[{job_id}|{pid}] {line}");
        }
        if let Ok(mut buffer) = buffer.lock() {
            let entry = LogLine {
                ts,
                job_id,
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
                job_id,
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

/// Current user
pub fn get_user_name() -> Option<String> {
    let system = System::new_with_specifics(
        RefreshKind::new()
            .with_processes(ProcessRefreshKind::new().with_user(UpdateKind::OnlyIfNotSet)),
    );
    let users = Users::new_with_refreshed_list();
    let pid = process::id();
    system
        .process(sysinfo::Pid::from_u32(pid))
        .and_then(|proc| {
            proc.effective_user_id()
                .or(proc.user_id())
                .and_then(|uid| users.get_user_by_id(uid))
                .map(|user| user.name().to_string())
        })
}
