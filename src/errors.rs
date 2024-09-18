use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DispatcherError {
    #[error("Failed to spawn process: {0}")]
    ProcSpawnError(io::Error),
    #[error("Failed to spawn process (timeout)")]
    ProcSpawnTimeoutError,
    #[error("Invalid command")]
    InvalidCommandError,
    #[error("Ping failed")]
    PingError,
    #[error("Command returned error")]
    CommandError,
    #[error("Communication error: {0}")]
    IpcClientError(#[from] IpcClientError),
}

#[derive(Error, Debug)]
pub enum IpcServerError {
    #[error("Failed to bind to socket: {0}")]
    BindError(io::Error),
    #[error("Failed to delete stale socket file: {0}")]
    FileError(io::Error),
    #[error("Failed to resolve socket name: {0}")]
    SocketNameError(io::Error),
    #[error("The socket is already in use by an instance of the current process.")]
    AlreadyInUseError,
}

#[derive(Error, Debug)]
pub enum IpcClientError {
    #[error("Failed to connect to socket: {0}")]
    ConnectError(#[from] io::Error),
    #[error("Failed to resolve socket name: {0}")]
    SocketNameError(io::Error),
    #[error("Failed to read from socket: {0}")]
    ReadError(#[from] IpcStreamReadError),
    #[error("Failed to write to socket: {0}")]
    WriteError(#[from] IpcStreamWriteError),
}

#[derive(Error, Debug)]
pub enum IpcStreamReadError {
    #[error("Failed to read from socket: {0}")]
    ReadError(#[from] io::Error),
    #[error("Failed to deserialize data from socket: {0}")]
    DeserializeError(#[from] bincode::Error),
}

#[derive(Error, Debug)]
pub enum IpcStreamWriteError {
    #[error("Failed to write to socket: {0}")]
    WriteError(#[from] io::Error),
    #[error("Failed to serialize data for socket: {0}")]
    SerializeError(#[from] bincode::Error),
}

#[derive(Error, Debug)]
pub enum IpcStreamError {
    #[error("Failed to read from socket: {0}")]
    ReadError(#[from] IpcStreamReadError),
    #[error("Failed to write to socket: {0}")]
    WriteError(#[from] IpcStreamWriteError),
}
