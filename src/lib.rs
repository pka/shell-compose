mod cli;
mod display;
mod ipc;
mod justfile;
mod spawner;

pub use cli::*;
pub use display::*;
pub use ipc::*;
pub use justfile::*;
pub use spawner::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DispatcherError {
    #[error(transparent)]
    CliArgsError(#[from] clap::Error),
    #[error("Failed to spawn process: {0}")]
    ProcSpawnError(std::io::Error),
    #[error("Failed to spawn process (timeout)")]
    ProcSpawnTimeoutError,
    #[error(transparent)]
    JustfileError(#[from] JustfileError),
    #[error("Invalid command")]
    InvalidCommandError,
    #[error("Command returned error")]
    CommandError,
    #[error(transparent)]
    IpcClientError(#[from] IpcClientError),
    #[error("Cron error: {0}")]
    CronError(#[from] cron::error::Error),
}

pub const SOCKET_NAME: &str = "shell-compose.sock";
