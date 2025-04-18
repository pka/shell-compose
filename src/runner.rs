use crate::{DispatcherError, Formatter, JobId, Pid, RestartInfo, RestartPolicy};
use chrono::{DateTime, Local, TimeDelta};
use command_group::{CommandGroup, GroupChild};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{self, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind, Users};

/// Child process controller
pub struct Runner {
    pub proc: GroupChild,
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

/// Required information for spawning a new process.
pub struct JobSpawnInfo {
    pub job_id: JobId,
    pub args: Vec<String>,
    pub restart_info: RestartInfo,
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
            // spawn process group (https://biriukov.dev/docs/fd-pipe-session-terminal/3-process-groups-jobs-and-sessions/)
            .group_spawn()
            .map_err(DispatcherError::ProcSpawnError)?;
        let pid = child.id();

        // output listeners
        let max_len = 200; // TODO: Make configurable
        let output = Arc::new(Mutex::new(OutputBuffer::new(Some(max_len))));

        let buffer = output.clone();
        let stdout = child.inner().stdout.take().unwrap();
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
        let stderr = child.inner().stderr.take().unwrap();
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
        info!("Terminating process {}", self.proc.id());
        self.proc.kill()?;
        Ok(())
    }
    pub fn restart_infos(&mut self) -> Option<JobSpawnInfo> {
        let respawn = !self.user_terminated
            && match self.restart_info.policy {
                RestartPolicy::Always => true,
                RestartPolicy::OnFailure => {
                    matches!(self.info.state, ProcStatus::ExitErr(code) if code > 0)
                }
                RestartPolicy::Never => false,
            };
        if respawn {
            let last_duration = self.info.end.unwrap_or(Local::now()) - self.info.start;
            let mut restart_info = self.restart_info.clone();
            if last_duration > TimeDelta::milliseconds(50) {
                // Reset wait time after a long run
                restart_info.wait_time = 50;
            } else {
                restart_info.wait_time *= 2;
            }
            Some(JobSpawnInfo {
                job_id: self.info.job_id,
                args: self.info.cmd_args.clone(),
                restart_info,
            })
        } else {
            None
        }
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
        RefreshKind::nothing()
            .with_processes(ProcessRefreshKind::nothing().with_user(UpdateKind::OnlyIfNotSet)),
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
