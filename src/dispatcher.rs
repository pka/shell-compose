use crate::{
    CliCommand, ExecCommand, IpcClientError, IpcStream, JobInfoMsg, Justfile, JustfileError,
    Message, ProcStatus, Runner,
};
use chrono::{DateTime, Local, TimeZone};
use job_scheduler_ng::{self as job_scheduler, JobScheduler};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;

pub(crate) type WatcherParam = u32; // PID

pub struct Dispatcher {
    procs: Arc<Mutex<Vec<Runner>>>,
    pub jobs: HashMap<u32, JobInfo>,
    /// Sender channel for Runner threads
    channel: mpsc::Sender<WatcherParam>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum JobInfo {
    Shell(Vec<String>),
    Service(String),
    Group(String),
    Cron(String, Vec<String>),
}

#[derive(Error, Debug)]
pub enum DispatcherError {
    #[error(transparent)]
    CliArgsError(#[from] clap::Error),
    #[error("Failed to spawn process: {0}")]
    ProcSpawnError(std::io::Error),
    #[error("Failed to spawn process (timeout)")]
    ProcSpawnTimeoutError,
    #[error("Failed to terminate child process: {0}")]
    KillError(std::io::Error),
    #[error("Process exit code: {0}")]
    ProcExitError(i32),
    #[error("Empty command")]
    EmptyProcCommandError,
    #[error(transparent)]
    JustfileError(#[from] JustfileError),
    #[error("Communication protocol error")]
    UnexpectedMessageError,
    #[error(transparent)]
    IpcClientError(#[from] IpcClientError),
    #[error("Cron error: {0}")]
    CronError(#[from] cron::error::Error),
}

impl Dispatcher {
    pub fn create() -> Self {
        let procs = Arc::new(Mutex::new(Vec::new()));
        let procs_watcher = procs.clone();
        let jobs = HashMap::new();
        let (send, recv) = mpsc::channel();
        let watcher_send = send.clone();
        let _watcher = thread::spawn(move || child_watcher(procs_watcher, watcher_send, recv));
        Dispatcher {
            procs,
            jobs,
            channel: send,
        }
    }
    pub fn exec_command(&mut self, cmd: ExecCommand) -> Message {
        info!("Executing `{cmd:?}`");
        let res = match cmd {
            ExecCommand::Run { args } => self.run(&args),
            ExecCommand::Runat { at, args } => self.run_at(&at, &args),
            ExecCommand::Start { service } => self.start(&service),
            ExecCommand::Up { group } => self.up(&group),
        };
        if let Err(e) = &res {
            error!("{e}");
        }
        res.into()
    }
    pub fn cli_command(&mut self, cmd: CliCommand, stream: &mut IpcStream) {
        info!("Executing `{cmd:?}`");
        let res = match cmd {
            CliCommand::Stop { pid } => self.stop(pid),
            CliCommand::Ps => self.ps(stream),
            CliCommand::Jobs => self.jobs(stream),
            CliCommand::Logs => self.log(stream),
            CliCommand::Exit => std::process::exit(0),
        };
        if let Err(e) = &res {
            error!("{e}");
        }
        let _ = stream.send_message(&res.into());
    }
    fn add_job(&mut self, job: JobInfo) -> u32 {
        let job_id = self.jobs.keys().max().unwrap_or(&0) + 1;
        self.jobs.insert(job_id, job);
        job_id
    }
    fn run(&mut self, args: &[String]) -> Result<(), DispatcherError> {
        let job_id = self.add_job(JobInfo::Shell(args.to_vec()));
        self.spawn_job(job_id, args)
    }
    fn spawn_job(&mut self, job_id: u32, args: &[String]) -> Result<(), DispatcherError> {
        let child = Runner::spawn(job_id, args, self.channel.clone())?;
        self.procs.lock().expect("lock").insert(0, child);
        // Wait for startup failure
        thread::sleep(Duration::from_millis(10));
        if let Ok(procs) = self.procs.lock() {
            if let Some(child) = procs.first() {
                return match child.info.state {
                    ProcStatus::ExitErr(code) => Err(DispatcherError::ProcExitError(code)),
                    // ProcStatus::Unknown(e) => Err(DispatcherError::ProcSpawnError(e)),
                    _ => Ok(()),
                };
            }
        }
        Ok(())
    }
    /// Stop process
    fn stop(&mut self, pid: u32) -> Result<(), DispatcherError> {
        if let Some(child) = self
            .procs
            .lock()
            .expect("lock")
            .iter_mut()
            .find(|p| p.info.pid == pid)
        {
            child.proc.kill().map_err(DispatcherError::KillError)?;
        }
        Ok(())
    }
    /// Add cron job
    fn run_at(&mut self, cron: &str, args: &[String]) -> Result<(), DispatcherError> {
        let job_id = self.add_job(JobInfo::Cron(cron.to_string(), args.to_vec()));
        let mut scheduler = JobScheduler::new();
        let job_args = args.to_vec();
        let procs = self.procs.clone();
        let channel = self.channel.clone();
        let _uuid = scheduler.add(job_scheduler::Job::new(cron.parse()?, move || {
            let child = Runner::spawn(job_id, &job_args, channel.clone()).unwrap();
            procs.lock().expect("lock").insert(0, child);
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
    /// Start service (just repipe)
    fn start(&mut self, service: &str) -> Result<(), DispatcherError> {
        let job_id = self.add_job(JobInfo::Service(service.to_string()));
        self.start_service(job_id, service)
    }
    fn start_service(&mut self, job_id: u32, service: &str) -> Result<(), DispatcherError> {
        self.spawn_job(
            job_id,
            vec!["just".to_string(), service.to_string()].as_slice(),
        )
    }
    /// Start service group (all just repipes in group)
    fn up(&mut self, group: &str) -> Result<(), DispatcherError> {
        let job_id = self.add_job(JobInfo::Group(group.to_string()));
        if let Ok(justfile) = Justfile::parse() {
            let recipes = justfile.group_recipes(group);
            for recipe in recipes {
                self.start_service(job_id, &recipe)?;
            }
        }
        Ok(())
    }
    /// Return info about running and finished processes
    fn ps(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        for child in &mut self.procs.lock().expect("lock").iter_mut() {
            let info = child.update_proc_info();
            if stream.send_message(&Message::PsInfo(info.clone())).is_err() {
                info!("Aborting ps command (stream error)");
                break;
            }
        }
        Ok(())
    }
    /// Return info about jobs
    fn jobs(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        for (id, info) in &mut self.jobs.iter() {
            let msg = JobInfoMsg {
                id: *id,
                info: info.clone(),
            };
            if stream.send_message(&Message::JobInfo(msg)).is_err() {
                info!("Aborting job command (stream error)");
                break;
            }
        }
        Ok(())
    }
    /// Return log lines
    fn log(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        let mut last_seen_ts: HashMap<u32, DateTime<Local>> = HashMap::new(); // pid -> last_seen
        'cmd: loop {
            for child in self.procs.lock().expect("lock").iter_mut() {
                // TODO: buffered log lines should be sorted by time instead by process+time
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
            }
            stream.alive()?;
            // Wait for new output
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

// sender: Sender channel for Runner threads
// recv: Watcher receiver channel
fn child_watcher(
    procs: Arc<Mutex<Vec<Runner>>>,
    sender: mpsc::Sender<WatcherParam>,
    recv: mpsc::Receiver<WatcherParam>,
) {
    loop {
        // PID of terminated process sent from output_listener
        let pid = recv.recv().unwrap();
        let ts = Local::now();
        info!(target: &format!("{pid}"), "Process terminated");
        if let Ok(mut procs) = procs.lock() {
            if let Some(child) = procs.iter_mut().find(|p| p.info.pid == pid) {
                // https://doc.rust-lang.org/std/process/struct.Child.html#warning
                let _ = child.proc.wait();
                let _ = child.update_proc_info();
                child.info.end = Some(ts);
                let respawn = matches!(child.info.state, ProcStatus::ExitErr(code) if code > 0);
                if respawn {
                    thread::sleep(Duration::from_millis(50));
                    let result =
                        Runner::spawn(child.info.job_id, &child.info.cmd_args, sender.clone());
                    match result {
                        Ok(child) => procs.insert(0, child),
                        Err(e) => error!("Error trying to respawn failed process: {e}"),
                    }
                }
            } else {
                error!("PID {pid} of terminating process not found");
            }
        }
    }
}
