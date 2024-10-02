use crate::log_color;
use crate::{DispatcherError, IpcStream, Message};
use chrono::{DateTime, Local, SecondsFormat, TimeZone};
use job_scheduler_ng::{Job, JobScheduler};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct ChildProc {
    proc: Child,
    output: Arc<Mutex<OutputBuffer>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LogLine {
    pub ts: DateTime<Local>,
    pub pid: u32,
    pub line: String,
    pub error: bool,
}

impl LogLine {
    pub fn log(&self) {
        let dt = self.ts.to_rfc3339_opts(SecondsFormat::Secs, true);
        let pid = self.pid;
        let line = &self.line;
        let color = log_color(pid as usize, self.error);
        println!("{color}{dt} [{pid}] {line}{color:#}");
    }
}

struct OutputBuffer {
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PsInfo {
    pub pid: u32,
    pub state: String,
}

impl ChildProc {
    fn spawn(args: &[String]) -> Result<Self, DispatcherError> {
        let mut cmd = VecDeque::from(args.to_owned());
        let Some(exe) = cmd.pop_front() else {
            return Err(DispatcherError::InvalidCommandError);
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
        let _stdout_handle =
            thread::spawn(move || output_listener(BufReader::new(stdout), pid, false, buffer));

        let buffer = output.clone();
        let stderr = child.stderr.take().unwrap();
        let _stderr_handle =
            thread::spawn(move || output_listener(BufReader::new(stderr), pid, true, buffer));

        let child_proc = ChildProc {
            proc: child,
            output,
        };
        Ok(child_proc)
    }
    fn is_running(&mut self) -> bool {
        matches!(self.proc.try_wait(), Ok(None))
    }
}

impl Drop for ChildProc {
    fn drop(&mut self) {
        self.proc.kill().unwrap();
    }
}

fn output_listener<R: Read>(
    reader: BufReader<R>,
    pid: u32,
    error: bool,
    buffer: Arc<Mutex<OutputBuffer>>,
) {
    reader.lines().map_while(Result::ok).for_each(|line| {
        let ts = Local::now();
        if error {
            eprintln!("[{pid}] {line}");
        } else {
            println!("[{pid}] {line}");
        }
        if let Ok(mut buffer) = buffer.lock() {
            let entry = LogLine {
                ts,
                pid,
                error,
                line,
            };
            buffer.push(entry);
        }
    });
}

#[derive(Default)]
pub struct Spawner {
    procs: Arc<Mutex<Vec<ChildProc>>>,
}

impl Spawner {
    pub fn new() -> Self {
        Spawner::default()
    }
    pub fn run(&mut self, args: &[String]) -> Result<(), DispatcherError> {
        let child = ChildProc::spawn(args)?;
        self.procs.lock().unwrap().insert(0, child);
        Ok(())
    }
    pub fn run_at(&mut self, cron: &str, args: &[String]) -> Result<(), DispatcherError> {
        let mut scheduler = JobScheduler::new();
        let job: Vec<String> = args.into();
        let procs = self.procs.clone();
        scheduler.add(Job::new(cron.parse()?, move || {
            let child = ChildProc::spawn(&job).unwrap();
            procs.lock().unwrap().insert(0, child);
        }));
        let _handle = thread::spawn(move || loop {
            // Should we use same scheduler and thread for all cron jobs?
            scheduler.tick();
            let wait_time = scheduler.time_till_next_job();
            if wait_time == Duration::from_millis(0) {
                // no future execution time -> exit
                info!("Ending cron job");
                break;
            }
            std::thread::sleep(wait_time);
        });
        Ok(())
    }
    pub fn ps(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        for child in &mut self.procs.lock().unwrap().iter_mut() {
            let state = match child.proc.try_wait() {
                Ok(Some(status)) => format!("Exited with {status}"),
                Ok(None) => "Running".to_string(),
                Err(e) => format!("Error {e}"),
            };
            let info = PsInfo {
                pid: child.proc.id(),
                state,
            };
            if stream.send_message(&Message::PsInfo(info)).is_err() {
                info!("Aborting ps command (stream error)");
                break;
            }
        }
        Ok(())
    }
    pub fn log(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        let mut last_seen_ts: HashMap<u32, DateTime<Local>> = HashMap::new(); // pid -> last_seen
        'cmd: loop {
            let mut running_childs = 0;
            for child in self.procs.lock().unwrap().iter_mut() {
                if let Ok(output) = child.output.lock() {
                    let last_seen = last_seen_ts
                        .entry(child.proc.id())
                        .or_insert(Local.timestamp_opt(0, 0).unwrap());
                    for entry in output.lines_since(last_seen) {
                        if stream
                            .send_message(&Message::LogLine(entry.clone()))
                            .is_err()
                        {
                            info!("Aborting log command (stream error)");
                            break 'cmd;
                        }
                    }
                }
                if child.is_running() {
                    running_childs += 1
                }
            }
            // End following logs when no process is running
            if running_childs == 0 {
                break;
            }
            // Wait for new output
            thread::sleep(Duration::from_millis(100));
        }
        stream.send_message(&Message::Ok)?;
        Ok(())
    }
}
