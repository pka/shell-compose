use crate::{IpcClientError, IpcServerError, IpcStreamReadError, IpcStreamWriteError, Message};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io;
use std::io::prelude::*;

/// Listen for incoming connections on the given socket.
///
/// # Arguments
///
/// * `socket` - The socket name to listen on.
/// * `handle_connection` - A function that will be invoked for each incoming connection.
/// * `handle_error` - An optional function that will be invoked if there is an error accepting a connection.
pub fn start_ipc_listener<F: FnMut(IpcStream) + Send + 'static>(
    socket: &str,
    mut on_connection: F,
    on_connection_error: Option<fn(io::Error)>,
) -> Result<(), IpcServerError> {
    let name = socket
        .to_ns_name::<GenericNamespaced>()
        .map_err(IpcServerError::SocketNameError)?;
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

    for stream in listener.incoming().filter_map(error_handler) {
        let stream = IpcStream { stream };
        on_connection(stream);
    }

    Ok(())
}

/// Connect to the socket and return the stream.
fn ipc_client_connect(socket_name: &str) -> Result<LocalSocketStream, IpcClientError> {
    let name = socket_name
        .to_ns_name::<GenericNamespaced>()
        .map_err(IpcClientError::SocketNameError)?;
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

/// Communication stream
pub struct IpcStream {
    stream: LocalSocketStream,
}

impl IpcStream {
    /// Connects to the socket and return the stream
    /// CAUTION: After connecting, only one message is currently supported
    pub fn connect(socket_name: &str) -> Result<Self, IpcClientError> {
        let stream = ipc_client_connect(socket_name)?;
        Ok(IpcStream { stream })
    }
    pub fn check_connection(socket_name: &str) -> Result<(), IpcClientError> {
        IpcStream::connect(socket_name)?.send_message(&Message::NoCommand)?;
        Ok(())
    }
    /// Send serializable object.
    pub fn send_message<TRequest: Serialize>(
        &mut self,
        request: &TRequest,
    ) -> Result<(), IpcClientError> {
        self.stream.write_serde(&request)?;
        Ok(())
    }
    /// Receive serializable object as response.
    pub fn receive_message<TResponse: DeserializeOwned>(
        &mut self,
    ) -> Result<TResponse, IpcClientError> {
        let response: TResponse = self.stream.read_serde()?;
        Ok(response)
    }
    /// Send a serializable object and immediately read a deserializable object from it,
    /// blocking until a response is received. Meant to be used for requests that expect a response from the server.
    pub fn send_query<TRequest: Serialize, TResponse: DeserializeOwned>(
        &mut self,
        request: &TRequest,
    ) -> Result<TResponse, IpcClientError> {
        self.send_message(&request)?;
        let response: TResponse = self.receive_message()?;
        Ok(response)
    }
}
