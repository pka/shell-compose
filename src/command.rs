use crate::{DispatcherError, Job, JobId, LogLine, ProcInfo};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli;

/// Shared commands with background service
#[derive(Subcommand, Debug, Serialize, Deserialize)]
pub enum ExecCommand {
    /// Execute shell command
    Run {
        /// Command arguments
        args: Vec<String>,
    },
    /// Execute shell command with cron schedule
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
    // Stop service group
    // Down {
    //     /// Service group name
    //     group: String,
    // },
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
    Logs,
    /// Stop all processes
    Exit,
}

/// IPC messages
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Connect,
    ExecCommand(ExecCommand),
    CliCommand(CliCommand),
    PsInfo(ProcInfo),
    JobInfo(Job),
    LogLine(LogLine),
    Ok,
    JobStarted(JobId),
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
