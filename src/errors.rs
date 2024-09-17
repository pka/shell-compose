use std::{any::Any, io};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IpcServerError {
    #[error("Failed to bind to socket: {0}")]
    BindError(io::Error),
    #[error("Failed to delete stale socket file: {0}")]
    FileError(io::Error),
    #[error("The socket is already in use by an instance of the current process.")]
    AlreadyInUseError,
    #[error("Failed to spawn server thread: {0}")]
    ThreadSpawnError(io::Error),
    #[error("Failed to join server thread")]
    ThreadJoinError(Box<dyn Any + Send + 'static>),
}

#[derive(Error, Debug)]
pub enum IpcClientError {
    #[error("Failed to connect to socket: {0}")]
    ConnectError(#[from] io::Error),
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
