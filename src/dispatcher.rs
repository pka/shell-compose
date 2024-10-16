use crate::{
    CliCommand, ExecCommand, IpcClientError, IpcStream, Justfile, JustfileError, Message,
    ProcStatus, Runner,
};
use chrono::{DateTime, Local, TimeZone};
use job_scheduler_ng::{self as job_scheduler, JobScheduler};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use thiserror::Error;

pub type JobId = u32;
pub type Pid = u32;

pub struct Dispatcher<'a> {
    jobs: BTreeMap<JobId, JobInfo>,
    last_job_id: JobId,
    cronjobs: HashMap<JobId, job_scheduler::Uuid>,
    procs: Arc<Mutex<Vec<Runner>>>,
    scheduler: Arc<Mutex<JobScheduler<'a>>>,
    system: System,
    /// Sender channel for Runner threads
    channel: mpsc::Sender<Pid>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct JobInfo {
    pub job_type: JobType,
    pub args: Vec<String>,
    pub entrypoint: Option<String>,
    pub restart: RestartInfo,
    // stats: #Runs, #Success, #Restarts
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum JobType {
    Shell,
    Service(String),
    Cron(String),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RestartInfo {
    pub policy: Restart,
    /// Waiting time before restart in ms
    pub wait_time: u64,
}

/// Restart policy
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Restart {
    Always,
    OnFailure,
    Never,
}

struct JobSpawnInfo<'a> {
    job_id: JobId,
    args: &'a [String],
    restart_info: RestartInfo,
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
    #[error("Service `{0}` not found")]
    ServiceNotFoundError(String),
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

impl Default for RestartInfo {
    fn default() -> Self {
        RestartInfo {
            policy: Restart::OnFailure,
            wait_time: 50,
        }
    }
}

impl JobInfo {
    pub fn new_shell_job(args: Vec<String>) -> Self {
        JobInfo {
            job_type: JobType::Shell,
            args,
            entrypoint: None,
            restart: RestartInfo {
                policy: Restart::Never,
                ..Default::default()
            },
        }
    }
    pub fn new_cron_job(cron: String, args: Vec<String>) -> Self {
        JobInfo {
            job_type: JobType::Cron(cron),
            args,
            entrypoint: None,
            restart: RestartInfo {
                policy: Restart::Never,
                ..Default::default()
            },
        }
    }
    pub fn new_service(service: String) -> Self {
        JobInfo {
            job_type: JobType::Service(service.clone()),
            args: vec!["just".to_string(), service], // TODO: exclude entrypoint
            entrypoint: Some("just".to_string()),
            restart: RestartInfo::default(),
        }
    }
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

        let system = System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::new()),
        );

        Dispatcher {
            jobs: BTreeMap::new(),
            last_job_id: 0,
            cronjobs: HashMap::new(),
            procs,
            scheduler,
            system,
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
            CliCommand::Logs { job_or_service } => self.log(job_or_service, stream),
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
    fn spawn_info(&self, job_id: JobId) -> Result<JobSpawnInfo<'_>, DispatcherError> {
        let job = self
            .jobs
            .get(&job_id)
            .ok_or(DispatcherError::JobNotFoundError(job_id))?;
        Ok(JobSpawnInfo {
            job_id,
            args: &job.args,
            restart_info: job.restart.clone(),
        })
    }
    /// Find service job
    fn find_job(&self, service: &str) -> Option<JobId> {
        self.jobs
            .iter()
            .find(|(_id, info)| matches!(&info.job_type, JobType::Service(name) if name == service))
            .map(|(id, _info)| *id)
    }
    fn run(&mut self, args: &[String]) -> Result<Vec<JobId>, DispatcherError> {
        let job_info = JobInfo::new_shell_job(args.to_vec());
        let job_id = self.add_job(job_info);
        self.spawn_job(job_id)?;
        Ok(vec![job_id])
    }
    fn spawn_job(&mut self, job_id: JobId) -> Result<(), DispatcherError> {
        let job = self.spawn_info(job_id)?;
        let child = Runner::spawn(job.job_id, job.args, job.restart_info, self.channel.clone())?;
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
                child.user_terminated = true;
                child.terminate().map_err(DispatcherError::KillError)?;
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
        let job_info = JobInfo::new_cron_job(cron.to_string(), args.to_vec());
        let restart_info = job_info.restart.clone();
        let job_id = self.add_job(job_info);
        let job_args = args.to_vec();
        let procs = self.procs.clone();
        let channel = self.channel.clone();
        let uuid = self
            .scheduler
            .lock()
            .expect("lock")
            .add(job_scheduler::Job::new(cron.parse()?, move || {
                let child = Runner::spawn(job_id, &job_args, restart_info.clone(), channel.clone())
                    .unwrap();
                procs.lock().expect("lock").push(child);
            }));
        self.cronjobs.insert(job_id, uuid);
        Ok(vec![job_id])
    }
    /// Start service (just recipe)
    fn start(&mut self, service: &str) -> Result<Vec<JobId>, DispatcherError> {
        // Find existing job or add new
        let job_id = self
            .find_job(service)
            .unwrap_or_else(|| self.add_job(JobInfo::new_service(service.to_string())));
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
            self.spawn_job(job_id)?;
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
                .filter(|(_id, info)| matches!(&info.job_type, JobType::Service(name) if *name == service))
                .for_each(|(id, _info)| job_ids.push(*id));
        }
        for job_id in job_ids {
            self.stop(job_id)?;
        }
        Ok(())
    }
    /// Return info about running and finished processes
    fn ps(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        // Update system info
        // For accurate CPU usage, a process needs to be refreshed twice
        // https://docs.rs/sysinfo/latest/i686-pc-windows-msvc/sysinfo/struct.Process.html#method.cpu_usage
        let ts = Local::now();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::new().with_cpu(),
        );
        // Collect pids and child pids
        let pids: Vec<sysinfo::Pid> = self
            .procs
            .lock()
            .expect("lock")
            .iter()
            .flat_map(|proc| {
                let parent_pid = sysinfo::Pid::from(proc.info.pid as usize);
                self.system
                    .processes()
                    .iter()
                    .filter(move |(_pid, process)| {
                        process.parent().unwrap_or(0.into()) == parent_pid
                    })
                    .map(|(pid, _process)| *pid)
                    .chain([parent_pid])
            })
            .collect();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL); // 200ms
        let duration = (Local::now() - ts).num_milliseconds();
        fn per_second(value: u64, ms: i64) -> u64 {
            (value as f64 * 1000.0 / ms as f64) as u64
        }
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            true,
            ProcessRefreshKind::new()
                .with_cpu()
                .with_disk_usage()
                .with_memory(),
        );

        let mut proc_infos = Vec::new();
        for child in &mut self.procs.lock().expect("lock").iter_mut().rev() {
            let parent_pid = sysinfo::Pid::from(child.info.pid as usize);
            // CPU usage has to be measured from just child processs
            // For tasks spawning child processes, we should consider the whole process subtree!
            let main_pid = if child.info.program() == "just" {
                self.system
                    .processes()
                    .iter()
                    .find(|(_pid, process)| {
                        process.parent().unwrap_or(0.into()) == parent_pid
                            && process.name() != "ctrl-c"
                    })
                    .map(|(pid, _process)| *pid)
                    .unwrap_or(parent_pid)
            } else {
                parent_pid
            };
            if let Some(process) = self.system.process(main_pid) {
                child.info.cpu = process.cpu_usage();
                child.info.memory = process.memory();
                child.info.virtual_memory = process.virtual_memory();
                let disk = process.disk_usage();
                child.info.total_written_bytes = disk.total_written_bytes;
                child.info.written_bytes = per_second(disk.written_bytes, duration);
                child.info.total_read_bytes = disk.total_read_bytes;
                child.info.read_bytes = per_second(disk.read_bytes, duration);
            } else {
                child.info.cpu = 0.0;
                child.info.memory = 0;
                child.info.virtual_memory = 0;
                child.info.written_bytes = 0;
                child.info.read_bytes = 0;
            }
            let info = child.update_proc_state();
            proc_infos.push(info.clone());
        }
        stream.send_message(&Message::PsInfo(proc_infos))?;
        Ok(())
    }
    /// Return info about jobs
    fn jobs(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        let mut job_infos = Vec::new();
        for (id, info) in self.jobs.iter().rev() {
            job_infos.push(Job {
                id: *id,
                info: info.clone(),
            });
        }
        stream.send_message(&Message::JobInfo(job_infos))?;
        Ok(())
    }
    /// Return log lines
    fn log(
        &mut self,
        job_or_service: Option<String>,
        stream: &mut IpcStream,
    ) -> Result<(), DispatcherError> {
        let mut job_id_filter = None;
        if let Some(job_or_service) = job_or_service {
            if let Ok(job_id) = JobId::from_str(&job_or_service) {
                if self.jobs.contains_key(&job_id) {
                    job_id_filter = Some(job_id);
                } else {
                    return Err(DispatcherError::JobNotFoundError(job_id));
                }
            } else {
                job_id_filter = Some(
                    self.find_job(&job_or_service)
                        .ok_or(DispatcherError::ServiceNotFoundError(job_or_service))?,
                );
            }
        }

        let mut last_seen_ts: HashMap<Pid, DateTime<Local>> = HashMap::new();
        'logwait: loop {
            // Collect log entries from child proceses
            let mut log_lines = Vec::new();
            for child in self.procs.lock().expect("lock").iter_mut() {
                if let Ok(output) = child.output.lock() {
                    let last_seen = last_seen_ts
                        .entry(child.proc.id())
                        .or_insert(Local.timestamp_millis_opt(0).single().expect("ts"));
                    for entry in output.lines_since(last_seen) {
                        if let Some(job_id) = job_id_filter {
                            if entry.job_id != job_id {
                                continue;
                            }
                        }
                        log_lines.push(entry.clone());
                    }
                }
            }

            if log_lines.is_empty() {
                // Exit when client is disconnected
                stream.alive()?;
            } else {
                log_lines.sort_by_key(|entry| entry.ts);
                for entry in log_lines {
                    if stream.send_message(&Message::LogLine(entry)).is_err() {
                        info!("Aborting log command (stream error)");
                        break 'logwait;
                    }
                }
            }
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
        let pid = recv.recv().expect("recv");
        let ts = Local::now();
        let mut respawn_child = None;
        if let Some(child) = procs
            .lock()
            .expect("lock")
            .iter_mut()
            .find(|p| p.info.pid == pid)
        {
            // https://doc.rust-lang.org/std/process/struct.Child.html#warning
            let exit_code = child.proc.wait().ok().and_then(|st| st.code());
            let _ = child.update_proc_state();
            child.info.end = Some(ts);
            if let Some(code) = exit_code {
                info!(target: &format!("{pid}"), "Process terminated with exit code {code}");
            } else {
                info!(target: &format!("{pid}"), "Process terminated");
            }
            let respawn = !child.user_terminated
                && match child.restart_info.policy {
                    Restart::Always => true,
                    Restart::OnFailure => {
                        matches!(child.info.state, ProcStatus::ExitErr(code) if code > 0)
                    }
                    Restart::Never => false,
                };
            if respawn {
                respawn_child = Some((child.info.clone(), child.restart_info.clone()));
            }
        } else {
            info!(target: &format!("{pid}"), "(Unknown) process terminated");
        }
        if let Some((child_info, restart_info)) = respawn_child {
            thread::sleep(Duration::from_millis(restart_info.wait_time));
            let result = Runner::spawn(
                child_info.job_id,
                &child_info.cmd_args,
                restart_info,
                sender.clone(),
            );
            match result {
                Ok(child) => procs.lock().expect("lock").push(child),
                Err(e) => error!("Error trying to respawn failed process: {e}"),
            }
        }
    }
}
