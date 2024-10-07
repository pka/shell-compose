use crate::{
    CliCommand, ExecCommand, IpcClientError, IpcStream, Justfile, JustfileError, Message,
    ProcStatus, Runner,
};
use chrono::{DateTime, Local, TimeZone};
use job_scheduler_ng::{self as job_scheduler, JobScheduler};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;

pub type JobId = u32;
pub type Pid = u32;

pub struct Dispatcher<'a> {
    procs: Arc<Mutex<Vec<Runner>>>,
    scheduler: Arc<Mutex<JobScheduler<'a>>>,
    pub jobs: BTreeMap<JobId, JobInfo>,
    cronjobs: HashMap<JobId, job_scheduler::Uuid>,
    /// Sender channel for Runner threads
    channel: mpsc::Sender<Pid>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum JobInfo {
    Shell(Vec<String>),
    Service(String),
    Group(String),
    Cron(String, Vec<String>),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Job {
    pub id: JobId,
    pub info: JobInfo,
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
    #[error("Job {0} not found")]
    JobNotFoundError(JobId),
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

impl Dispatcher<'_> {
    pub fn create() -> Dispatcher<'static> {
        let procs = Arc::new(Mutex::new(Vec::new()));
        let scheduler = Arc::new(Mutex::new(JobScheduler::new()));

        let scheduler_spawn = scheduler.clone();
        let _handle = thread::spawn(move || cron_scheduler(scheduler_spawn));

        let (send, recv) = mpsc::channel();
        let send_spawn = send.clone();
        let procs_spawn = procs.clone();
        let _watcher = thread::spawn(move || child_watcher(procs_spawn, send_spawn, recv));

        Dispatcher {
            procs,
            scheduler,
            jobs: BTreeMap::new(),
            cronjobs: HashMap::new(),
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
        match res {
            Err(e) => {
                error!("{e}");
                Message::Err(format!("{e}"))
            }
            Ok(job_id) => Message::JobStarted(job_id),
        }
    }
    pub fn cli_command(&mut self, cmd: CliCommand, stream: &mut IpcStream) {
        info!("Executing `{cmd:?}`");
        let res = match cmd {
            CliCommand::Stop { job_id } => self.stop(job_id),
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
    fn add_job(&mut self, job: JobInfo) -> JobId {
        let job_id = self.jobs.keys().max().unwrap_or(&0) + 1;
        self.jobs.insert(job_id, job);
        job_id
    }
    fn run(&mut self, args: &[String]) -> Result<JobId, DispatcherError> {
        let job_id = self.add_job(JobInfo::Shell(args.to_vec()));
        self.spawn_job(job_id, args)?;
        Ok(job_id)
    }
    fn spawn_job(&mut self, job_id: JobId, args: &[String]) -> Result<(), DispatcherError> {
        let child = Runner::spawn(job_id, args, self.channel.clone())?;
        self.procs.lock().expect("lock").push(child);
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
    fn stop(&mut self, job_id: JobId) -> Result<(), DispatcherError> {
        if let Some(uuid) = self.cronjobs.remove(&job_id) {
            info!("Removing cron job {job_id}");
            self.scheduler.lock().expect("lock").remove(uuid);
        }
        for child in self
            .procs
            .lock()
            .expect("lock")
            .iter_mut()
            .filter(|p| p.info.job_id == job_id)
            .filter(|p| !p.info.state.exited())
        {
            info!("Terminating process {}", child.proc.id());
            child.proc.kill().map_err(DispatcherError::KillError)?;
        }
        if self.jobs.remove(&job_id).is_some() {
            Ok(())
        } else {
            Err(DispatcherError::JobNotFoundError(job_id))
        }
    }
    /// Add cron job
    fn run_at(&mut self, cron: &str, args: &[String]) -> Result<JobId, DispatcherError> {
        let job_id = self.add_job(JobInfo::Cron(cron.to_string(), args.to_vec()));
        let job_args = args.to_vec();
        let procs = self.procs.clone();
        let channel = self.channel.clone();
        let uuid = self
            .scheduler
            .lock()
            .expect("lock")
            .add(job_scheduler::Job::new(cron.parse()?, move || {
                let child = Runner::spawn(job_id, &job_args, channel.clone()).unwrap();
                procs.lock().expect("lock").push(child);
            }));
        self.cronjobs.insert(job_id, uuid);
        Ok(job_id)
    }
    /// Start service (just repipe)
    fn start(&mut self, service: &str) -> Result<JobId, DispatcherError> {
        let job_id = self.add_job(JobInfo::Service(service.to_string()));
        self.start_service(job_id, service)?;
        Ok(job_id)
    }
    fn start_service(&mut self, job_id: JobId, service: &str) -> Result<(), DispatcherError> {
        self.spawn_job(
            job_id,
            vec!["just".to_string(), service.to_string()].as_slice(),
        )
    }
    /// Start service group (all just repipes in group)
    fn up(&mut self, group: &str) -> Result<JobId, DispatcherError> {
        let job_id = self.add_job(JobInfo::Group(group.to_string()));
        if let Ok(justfile) = Justfile::parse() {
            let recipes = justfile.group_recipes(group);
            for recipe in recipes {
                self.start_service(job_id, &recipe)?;
            }
        }
        Ok(job_id)
    }
    /// Return info about running and finished processes
    fn ps(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        for child in &mut self.procs.lock().expect("lock").iter_mut().rev() {
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
        for (id, info) in self.jobs.iter().rev() {
            let job = Job {
                id: *id,
                info: info.clone(),
            };
            if stream.send_message(&Message::JobInfo(job)).is_err() {
                info!("Aborting job command (stream error)");
                break;
            }
        }
        Ok(())
    }
    /// Return log lines
    fn log(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        let mut last_seen_ts: HashMap<Pid, DateTime<Local>> = HashMap::new();
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

fn cron_scheduler(scheduler: Arc<Mutex<JobScheduler<'static>>>) {
    loop {
        let wait_time = if let Ok(mut scheduler) = scheduler.lock() {
            scheduler.tick();
            scheduler.time_till_next_job()
        } else {
            Duration::from_millis(50)
        };
        std::thread::sleep(wait_time);
    }
}

// sender: Sender channel for Runner threads
// recv: Watcher receiver channel
fn child_watcher(
    procs: Arc<Mutex<Vec<Runner>>>,
    sender: mpsc::Sender<Pid>,
    recv: mpsc::Receiver<Pid>,
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
                        Ok(child) => procs.push(child),
                        Err(e) => error!("Error trying to respawn failed process: {e}"),
                    }
                }
            } else {
                error!("PID {pid} of terminating process not found");
            }
        }
    }
}
