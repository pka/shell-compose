use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug, Serialize, Deserialize)]
pub enum Command {
    /// Execute shell command
    Run {
        /// Command arguments
        args: Vec<String>,
    },
    /// List running commands
    Ps,
    /// Show process logs
    Logs,
}

/// IPC messages
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Command(Command), // -> Some(Ok)
    NoCommand,        // -> None
    Ok,               // -> None
}

impl From<Cli> for Message {
    fn from(cli: Cli) -> Self {
        Message::Command(cli.command)
    }
}
