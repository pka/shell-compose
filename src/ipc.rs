use crate::{get_user_name, Message};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};
use log::debug;
use std::io;
use std::io::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IpcServerError {
    #[error("Failed to bind to socket: {0}")]
    BindError(io::Error),
    #[error("Failed to resolve socket name: {0}")]
    SocketNameError(io::Error),
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

/// Listen for incoming connections on the given socket.
///
/// # Arguments
///
/// * `socket` - The socket name to listen on.
/// * `on_connection` - A function that will be invoked for each incoming connection.
/// * `on_connection_error` - An optional function that will be invoked if there is an error accepting a connection.
pub fn start_ipc_listener<F: FnMut(IpcStream) + Send + 'static>(
    socket: &str,
    mut on_connection: F,
    on_connection_error: Option<fn(io::Error)>,
) -> Result<(), IpcServerError> {
    let name = socket
        .to_fs_name::<GenericFilePath>()
        .map_err(IpcServerError::SocketNameError)?;
    let mut options = ListenerOptions::new().name(name.clone());
    #[cfg(target_family = "unix")]
    {
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        options = options.mode(0o600);
    }
    #[cfg(target_family = "windows")]
    {
        use interprocess::os::windows::{
            local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
        };
        options = options.security_descriptor(SecurityDescriptor::new().unwrap());
    }
    let listener = match options.create_sync() {
        Err(e) => return Err(IpcServerError::BindError(e)),
        Ok(listener) => listener,
    };

    let error_handler = move |inc: Result<LocalSocketStream, io::Error>| match inc {
        Ok(conn) => Some(conn),
        Err(e) => {
            if let Some(on_connection_error) = on_connection_error {
                on_connection_error(e);
            }
            None
        }
    };

    for stream in listener.incoming().filter_map(error_handler) {
        let logname = "listener".to_string();
        let stream = IpcStream { logname, stream };
        on_connection(stream);
    }

    Ok(())
}

/// Connect to the socket and return the stream.
fn ipc_client_connect(socket_name: &str) -> Result<LocalSocketStream, IpcClientError> {
    let name = socket_name
        .to_fs_name::<GenericFilePath>()
        .map_err(IpcClientError::SocketNameError)?;
    LocalSocketStream::connect(name).map_err(IpcClientError::ConnectError)
}

trait SocketExt {
    fn read_serde<T: serde::de::DeserializeOwned>(&mut self) -> Result<T, IpcStreamReadError>;
    fn write_serde<T: serde::Serialize>(&mut self, data: &T) -> Result<(), IpcStreamWriteError>;
}

impl SocketExt for LocalSocketStream {
    /// Read a serializable object from the socket.
    ///
    /// This reads a `u32` in little endian, then reads that many bytes from the socket, then deserializes the data using `bincode::deserialize`.
    fn read_serde<T: serde::de::DeserializeOwned>(&mut self) -> Result<T, IpcStreamReadError> {
        let size = self.read_u32::<LittleEndian>()?;

        let bytes = {
            let mut bytes = vec![0; size as usize];

            self.read_exact(&mut bytes)?;

            bytes
        };

        let result: T = bincode::deserialize(&bytes)?;

        Ok(result)
    }

    /// Write a serializable object to the socket.
    ///
    /// This serializes the data using `bincode::serialize`, writes the length of the serialized data as a `u32` in little endian, then writes the serialized data.
    fn write_serde<T: serde::Serialize>(&mut self, data: &T) -> Result<(), IpcStreamWriteError> {
        let bytes = bincode::serialize(data)?;

        self.write_u32::<LittleEndian>(bytes.len() as u32)?;
        self.write_all(&bytes)?;

        Ok(())
    }
}

/// Communication stream
pub struct IpcStream {
    logname: String,
    stream: LocalSocketStream,
}

impl IpcStream {
    /// Connects to the socket and return the stream
    pub fn connect(logname: &str) -> Result<Self, IpcClientError> {
        let socket_name = IpcStream::user_socket_name();
        let mut stream = ipc_client_connect(&socket_name)?;
        stream.write_serde(&Message::Connect)?;
        Ok(IpcStream {
            logname: logname.to_string(),
            stream,
        })
    }
    /// Check socket connection
    pub fn check_connection() -> Result<(), IpcClientError> {
        IpcStream::connect("check_connection")?;
        Ok(())
    }
    pub fn user_socket_name() -> String {
        let user = get_user_name().unwrap_or("_".to_string());
        IpcStream::socket_name(&user)
    }
    #[cfg(target_family = "unix")]
    fn socket_name(user: &str) -> String {
        let tmpdir = std::env::var("TMPDIR").ok();
        format!(
            "{}/shell-compose-{user}.sock",
            tmpdir.as_deref().unwrap_or("/tmp")
        )
    }
    #[cfg(target_family = "windows")]
    fn socket_name(user: &str) -> String {
        format!(r"\\.\pipe\shell-compose-{user}")
    }
    /// Check stream
    pub fn alive(&mut self) -> Result<(), IpcClientError> {
        self.stream.write_serde(&Message::Connect)?;
        Ok(())
    }
    /// Send Message.
    pub fn send_message(&mut self, message: &Message) -> Result<(), IpcClientError> {
        debug!(target: &self.logname, "send_message {message:?}");
        self.stream.write_serde(&message)?;
        Ok(())
    }
    /// Receive Message.
    pub fn receive_message(&mut self) -> Result<Message, IpcClientError> {
        let message = self.stream.read_serde()?;
        debug!(target: &self.logname, "receive_message {message:?}");
        Ok(message)
    }
    /// Send a message and immediately read response message,
    /// blocking until a response is received.
    pub fn send_query(&mut self, request: &Message) -> Result<Message, IpcClientError> {
        self.send_message(request)?;
        let response = self.receive_message()?;
        Ok(response)
    }
}
