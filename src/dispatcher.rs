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
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use thiserror::Error;

pub type JobId = u32;
pub type Pid = u32;

pub struct Dispatcher<'a> {
    procs: Arc<Mutex<Vec<Runner>>>,
    scheduler: Arc<Mutex<JobScheduler<'a>>>,
    jobs: BTreeMap<JobId, JobInfo>,
    last_job_id: JobId,
    cronjobs: HashMap<JobId, job_scheduler::Uuid>,
    /// Sender channel for Runner threads
    channel: mpsc::Sender<Pid>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum JobInfo {
    Shell(Vec<String>),
    Service(String),
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
            last_job_id: 0,
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
            Ok(job_ids) => Message::JobsStarted(job_ids),
        }
    }
    pub fn cli_command(&mut self, cmd: CliCommand, stream: &mut IpcStream) {
        info!("Executing `{cmd:?}`");
        let res = match cmd {
            CliCommand::Stop { job_id } => self.stop(job_id),
            CliCommand::Down { group } => self.down(&group),
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
        self.last_job_id += 1;
        self.jobs.insert(self.last_job_id, job);
        self.last_job_id
    }
    fn run(&mut self, args: &[String]) -> Result<Vec<JobId>, DispatcherError> {
        let job_id = self.add_job(JobInfo::Shell(args.to_vec()));
        self.spawn_job(job_id, args)?;
        Ok(vec![job_id])
    }
    fn spawn_job(&mut self, job_id: JobId, args: &[String]) -> Result<(), DispatcherError> {
        let child = Runner::spawn(job_id, args, self.channel.clone())?;
        self.procs.lock().expect("lock").push(child);
        // Wait for startup failure
        thread::sleep(Duration::from_millis(10));
        if let Some(child) = self.procs.lock().expect("lock").last() {
            return match child.info.state {
                ProcStatus::ExitErr(code) => Err(DispatcherError::ProcExitError(code)),
                // ProcStatus::Unknown(e) => Err(DispatcherError::ProcSpawnError(e)),
                _ => Ok(()),
            };
        }
        Ok(())
    }
    /// Stop job
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
            .filter(|child| child.info.job_id == job_id)
        {
            if child.is_running() {
                if child.info.cmd_args.first().unwrap_or(&"".to_string()) == "just" {
                    // just does not propagate signals, so we have to kill its child process
                    let just_pid = child.proc.id() as usize;
                    let system = System::new_with_specifics(
                        RefreshKind::new().with_processes(ProcessRefreshKind::new()),
                    );
                    if let Some((pid, process)) =
                        system.processes().iter().find(|(_pid, process)| {
                            process.parent().unwrap_or(0.into()) == just_pid.into()
                                && process.name() != "ctrl-c"
                        })
                    {
                        info!("Terminating process {pid} (parent process {just_pid})");
                        process.kill(); // process.kill_with(Signal::Term)
                    }
                    // In an interactive terminal session, sending Ctrl-C terminates the running process.
                    // let mut stdin = child.proc.stdin.take().unwrap();
                    // stdin.write_all(&[3]).map_err(DispatcherError::KillError)?;
                } else {
                    info!("Terminating process {}", child.proc.id());
                    child.proc.kill().map_err(DispatcherError::KillError)?;
                }
            }
        }
        if self.jobs.remove(&job_id).is_some() {
            Ok(())
        } else {
            Err(DispatcherError::JobNotFoundError(job_id))
        }
    }
    /// Add cron job
    fn run_at(&mut self, cron: &str, args: &[String]) -> Result<Vec<JobId>, DispatcherError> {
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
        Ok(vec![job_id])
    }
    /// Start service (just recipe)
    fn start(&mut self, service: &str) -> Result<Vec<JobId>, DispatcherError> {
        // Find existing job or add new
        let job_id = self
            .jobs
            .iter()
            .find(|(_id, info)| matches!(info, JobInfo::Service(name) if name == service))
            .map(|(id, _info)| *id)
            .unwrap_or_else(|| self.add_job(JobInfo::Service(service.to_string())));
        // Check for existing process for this service
        let running = self
            .procs
            .lock()
            .expect("lock")
            .iter_mut()
            .any(|child| child.info.job_id == job_id && child.is_running());
        if running {
            Ok(vec![])
        } else {
            self.spawn_job(
                job_id,
                vec!["just".to_string(), service.to_string()].as_slice(),
            )?;
            Ok(vec![job_id])
        }
    }
    /// Start service group (all just repipes in group)
    fn up(&mut self, group: &str) -> Result<Vec<JobId>, DispatcherError> {
        let mut job_ids = Vec::new();
        let justfile = Justfile::parse()?;
        let recipes = justfile.group_recipes(group);
        for service in recipes {
            let ids = self.start(&service)?;
            job_ids.extend(ids);
        }
        Ok(job_ids)
    }
    /// Stop service group
    fn down(&mut self, group: &str) -> Result<(), DispatcherError> {
        let mut job_ids = Vec::new();
        let justfile = Justfile::parse()?;
        let recipes = justfile.group_recipes(group);
        for service in recipes {
            self.jobs
                .iter()
                .filter(|(_id, info)| matches!(info, JobInfo::Service(name) if *name == service))
                .for_each(|(id, _info)| job_ids.push(*id));
        }
        for job_id in job_ids {
            self.stop(job_id)?;
        }
        Ok(())
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
        if let Ok(mut procs) = procs.lock() {
            if let Some(child) = procs.iter_mut().find(|p| p.info.pid == pid) {
                // https://doc.rust-lang.org/std/process/struct.Child.html#warning
                let exit_code = child.proc.wait().ok().and_then(|st| st.code());
                let _ = child.update_proc_info();
                child.info.end = Some(ts);
                if let Some(code) = exit_code {
                    info!(target: &format!("{pid}"), "Process terminated with exit code {code}");
                } else {
                    info!(target: &format!("{pid}"), "Process terminated");
                }
                let mincode = if child.info.cmd_args.first().unwrap_or(&"".to_string()) == "just" {
                    // just exits with code 1 when child process is terminated (130 when ctrl-c handler exits)
                    1
                } else {
                    0
                };
                let respawn =
                    matches!(child.info.state, ProcStatus::ExitErr(code) if code > mincode);
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
                info!(target: &format!("{pid}"), "(Unknown) process terminated");
            }
        }
    }
}
