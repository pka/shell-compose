use crate::log_color;
use crate::DispatcherError;
use chrono::{DateTime, Local, SecondsFormat};
use job_scheduler_ng::{Job, JobScheduler};
use log::info;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct ChildProc {
    proc: Child,
    output: Arc<Mutex<OutputBuffer>>,
    /// Last output line displayed
    last_line: usize,
}

struct LogLine {
    ts: DateTime<Local>,
    line: String,
    error: bool,
}
type OutputBuffer = Vec<LogLine>;

impl LogLine {
    fn log(&self, pid: u32) {
        let dt = self.ts.to_rfc3339_opts(SecondsFormat::Secs, true);
        let line = &self.line;
        let color = log_color(pid as usize, self.error);
        println!("{color}{dt} [{pid}] {line}{color:#}");
    }
}

impl ChildProc {
    fn spawn(args: &[String]) -> Result<Self, DispatcherError> {
        let mut cmd = VecDeque::from(args.to_owned());
        let Some(exe) = cmd.pop_front() else {
            return Err(DispatcherError::InvalidCommandError);
        };
        // info!(target: "dispatcher", "Spawning {exe} {cmd:?}");

        let mut child = Command::new(exe)
            .args(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(DispatcherError::ProcSpawnError)?;

        // output listeners
        let output = Arc::new(Mutex::new(OutputBuffer::new()));

        let buffer = output.clone();
        let stdout = child.stdout.take().unwrap();
        let _stdout_handle =
            thread::spawn(move || output_listener(BufReader::new(stdout), false, buffer));

        let buffer = output.clone();
        let stderr = child.stderr.take().unwrap();
        let _stderr_handle =
            thread::spawn(move || output_listener(BufReader::new(stderr), true, buffer));

        let child_proc = ChildProc {
            proc: child,
            output,
            last_line: 0,
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

fn output_listener<R: Read>(reader: BufReader<R>, error: bool, buffer: Arc<Mutex<OutputBuffer>>) {
    reader.lines().map_while(Result::ok).for_each(|line| {
        if let Ok(mut buffer) = buffer.lock() {
            let entry = LogLine {
                ts: Local::now(),
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
    pub fn ps(&mut self) -> Result<(), DispatcherError> {
        for child in &mut self.procs.lock().unwrap().iter_mut() {
            let state = match child.proc.try_wait() {
                Ok(Some(status)) => format!("Exited with {status}"),
                Ok(None) => "Running".to_string(),
                Err(e) => format!("Error {e}"),
            };
            println!("PID: {} - {state}", child.proc.id());
        }
        Ok(())
    }
    pub fn log(&mut self) -> Result<(), DispatcherError> {
        loop {
            let mut running_childs = 0;
            for child in self.procs.lock().unwrap().iter_mut() {
                if let Ok(output) = child.output.lock() {
                    if output.len() > child.last_line {
                        let pid = child.proc.id();
                        for entry in output.iter().skip(child.last_line) {
                            entry.log(pid);
                        }
                        child.last_line = output.len();
                    }
                }
                if child.is_running() {
                    running_childs += 1
                }
            }
            if running_childs == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}
