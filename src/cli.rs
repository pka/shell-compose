use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli;

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
}

#[derive(Subcommand, Debug, Serialize, Deserialize)]
pub enum QueryCommand {
    /// List running commands
    Ps,
    /// Show process logs
    Logs,
}

/// IPC messages
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    ExecCommand(ExecCommand),   // -> Some(Ok)
    QueryCommand(QueryCommand), // -> Some(Ok)
    NoCommand,                  // -> None
    Ok,                         // -> None
}

impl From<ExecCommand> for Message {
    fn from(cmd: ExecCommand) -> Self {
        Message::ExecCommand(cmd)
    }
}

impl From<QueryCommand> for Message {
    fn from(cmd: QueryCommand) -> Self {
        Message::QueryCommand(cmd)
    }
}
