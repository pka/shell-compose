use crate::{
    ExecCommand, IpcClientError, IpcStream, Justfile, JustfileError, Message, ProcStatus,
    QueryCommand, Runner,
};
use chrono::{DateTime, Local, TimeZone};
use job_scheduler_ng::{Job, JobScheduler};
use log::{error, info};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Default)]
pub struct Dispatcher {
    procs: Arc<Mutex<Vec<Runner>>>,
}

#[derive(Error, Debug)]
pub enum DispatcherError {
    #[error(transparent)]
    CliArgsError(#[from] clap::Error),
    #[error("Failed to spawn process: {0}")]
    ProcSpawnError(std::io::Error),
    #[error("Failed to spawn process (timeout)")]
    ProcSpawnTimeoutError,
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
    pub fn new() -> Self {
        Dispatcher::default()
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
    pub fn query_command(&mut self, cmd: QueryCommand, stream: &mut IpcStream) {
        info!("Executing `{cmd:?}`");
        let res = match cmd {
            QueryCommand::Exit => std::process::exit(0),
            QueryCommand::Ps => self.ps(stream),
            QueryCommand::Logs => self.log(stream),
        };
        if let Err(e) = &res {
            error!("{e}");
        }
        let _ = stream.send_message(&res.into());
    }
    /// Spawn command
    fn run(&mut self, args: &[String]) -> Result<(), DispatcherError> {
        let mut child = Runner::spawn(args)?;
        // Wait for startup failure
        thread::sleep(Duration::from_millis(10));
        let result = match child.update_proc_info().state {
            ProcStatus::ExitErr(code) => Err(DispatcherError::ProcExitError(code)),
            // ProcStatus::Unknown(e) => Err(DispatcherError::ProcSpawnError(e)),
            _ => Ok(()),
        };
        self.procs.lock().unwrap().insert(0, child);
        result
    }
    /// Add cron job for spawning command
    fn run_at(&mut self, cron: &str, args: &[String]) -> Result<(), DispatcherError> {
        let mut scheduler = JobScheduler::new();
        let job: Vec<String> = args.into();
        let procs = self.procs.clone();
        scheduler.add(Job::new(cron.parse()?, move || {
            let child = Runner::spawn(&job).unwrap();
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
    /// Start service (just repipe)
    fn start(&mut self, service: &str) -> Result<(), DispatcherError> {
        self.run(vec!["just".to_string(), service.to_string()].as_slice())
    }
    /// Start service group (all just repipes in group)
    fn up(&mut self, group: &str) -> Result<(), DispatcherError> {
        if let Ok(justfile) = Justfile::parse() {
            let recipes = justfile.group_recipes(group);
            for recipe in recipes {
                self.start(&recipe)?;
            }
        }
        Ok(())
    }
    /// Return info about running and finished commands
    fn ps(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        for child in &mut self.procs.lock().unwrap().iter_mut() {
            let info = child.update_proc_info();
            if stream.send_message(&Message::PsInfo(info.clone())).is_err() {
                info!("Aborting ps command (stream error)");
                break;
            }
        }
        Ok(())
    }
    /// Return log lines
    fn log(&mut self, stream: &mut IpcStream) -> Result<(), DispatcherError> {
        let mut last_seen_ts: HashMap<u32, DateTime<Local>> = HashMap::new(); // pid -> last_seen
        'cmd: loop {
            for child in self.procs.lock().unwrap().iter_mut() {
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
