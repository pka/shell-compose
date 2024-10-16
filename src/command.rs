use crate::{DispatcherError, Job, JobId, LogLine, ProcInfo};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli;

/// Shared commands with background service
#[derive(Subcommand, Debug, Serialize, Deserialize)]
pub enum ExecCommand {
    /// Execute command
    Run {
        /// Command arguments
        args: Vec<String>,
    },
    /// Execute command with cron schedule
    Runat {
        /// Cron expression
        at: String,
        /// Command arguments
        args: Vec<String>,
    },
    /// Start service
    Start {
        /// Service name
        service: String,
    },
    /// Start service group
    Up {
        /// Service group name
        group: String,
    },
}

/// Additional commands
#[derive(Subcommand, Debug, Serialize, Deserialize)]
pub enum CliCommand {
    /// Stop service group
    Down {
        /// Service group name
        group: String,
    },
    /// Stop job
    Stop {
        /// Job id
        job_id: JobId,
    },
    /// List processes
    Ps,
    /// List active jobs
    Jobs,
    /// Show process logs
    Logs {
        /// Job id or service name
        job_or_service: Option<String>,
        // --tail: Option<usize>,
    },
    /// Stop all processes
    Exit,
}

/// IPC messages
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    // cli <-> Listener
    Connect,
    // cli -> Listener
    ExecCommand(ExecCommand),
    CliCommand(CliCommand),
    // cli <- Listener
    PsInfo(Vec<ProcInfo>),
    JobInfo(Vec<Job>),
    LogLine(LogLine),
    Ok,
    JobsStarted(Vec<JobId>),
    Err(String),
}

impl From<ExecCommand> for Message {
    fn from(cmd: ExecCommand) -> Self {
        Message::ExecCommand(cmd)
    }
}

impl From<CliCommand> for Message {
    fn from(cmd: CliCommand) -> Self {
        Message::CliCommand(cmd)
    }
}

/// Convert execution result into response message
impl From<Result<(), DispatcherError>> for Message {
    fn from(res: Result<(), DispatcherError>) -> Self {
        if let Err(e) = res {
            Message::Err(format!("{e}"))
        } else {
            Message::Ok
        }
    }
}
