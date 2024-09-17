use crate::{IpcClientError, IpcServerError, IpcStreamReadError, IpcStreamWriteError};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io;
use std::io::prelude::*;
use std::thread::JoinHandle;

/// Attempts to spin up a thread that will listen for incoming connections on the given socket.
///
/// It then creates a new thread where it will listen for incoming connections, and
/// invoke the passed `handle_connection` function.
///
/// # Arguments
///
/// * `socket` - The socket name to listen on.
/// * `handle_connection` - A function that will be invoked for each incoming connection.
/// * `handle_error` - An optional function that will be invoked if there is an error accepting a connection.
pub fn start_ipc_listener<F: Fn(LocalSocketStream) + Send + 'static>(
    socket: &str,
    on_connection: F,
    on_connection_error: Option<fn(io::Error)>,
) -> Result<JoinHandle<()>, IpcServerError> {
    let name = socket.to_ns_name::<GenericNamespaced>().unwrap();
    let listener = match ListenerOptions::new().name(name.clone()).create_sync() {
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

    let thread = std::thread::Builder::new()
        .name(format!("ipc server '{socket}'"))
        .spawn(move || {
            for stream in listener.incoming().filter_map(error_handler) {
                on_connection(stream);
            }
        })
        .map_err(IpcServerError::ThreadSpawnError)?;

    Ok(thread)
}

/// A wrapper around `start_ipc_listener`.
///
/// Rather than passing the LocalSocketStream directly to the `on_connection` callback,
/// this function instead reads a deserializable object from the socket and passes that, then optionally responds with a serializable object.
pub fn start_ipc_server<
    TRequest: DeserializeOwned,
    TResponse: Serialize,
    F: Fn(TRequest) -> Option<TResponse> + Send + 'static,
>(
    socket: &str,
    on_connection: F,
    on_connection_error: Option<fn(io::Error)>,
) -> Result<JoinHandle<()>, IpcServerError> {
    start_ipc_listener(
        socket,
        move |mut stream| {
            let request: TRequest = stream.read_serde().unwrap();

            if let Some(response) = on_connection(request) {
                stream.write_serde(&response).unwrap();
            }
        },
        on_connection_error,
    )
}

/// Connects to the socket and writes a serializable object to it.
/// Meant to be used for requests that don't expect a response from the server.
pub fn send_ipc_message<TRequest: Serialize>(
    socket_name: &str,
    request: &TRequest,
) -> Result<(), IpcClientError> {
    let mut stream = ipc_client_connect(socket_name)?;
    stream.write_serde(&request)?;
    Ok(())
}

/// Connect to the socket and write a serializable object to it, then immediately read a deserializable object from it,
/// blocking until a response is received. Meant to be used for requests that expect a response from the server.
pub fn send_ipc_query<TRequest: Serialize, TResponse: DeserializeOwned>(
    socket_name: &str,
    request: &TRequest,
) -> Result<TResponse, IpcClientError> {
    let mut stream = ipc_client_connect(socket_name)?;
    stream.write_serde(&request)?;
    let response: TResponse = stream.read_serde()?;
    Ok(response)
}

/// Connects to the socket and returns the stream.
pub fn ipc_client_connect(socket_name: &str) -> Result<LocalSocketStream, IpcClientError> {
    let name = socket_name.to_ns_name::<GenericNamespaced>().unwrap();
    LocalSocketStream::connect(name).map_err(IpcClientError::ConnectError)
}

pub trait SocketExt {
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
