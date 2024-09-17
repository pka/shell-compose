use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Stream};
use std::io::{self, prelude::*, BufReader};

struct Listener;

impl Listener {
    const SOCKET_NAME: &'static str = "process-dispatcher.sock";
    // Function that checks for errors in incoming connections. We'll use this to filter
    // through connections that fail on initialization for one reason or another.
    fn handle_error(conn: io::Result<Stream>) -> Option<Stream> {
        match conn {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Incoming connection failed: {e}");
                None
            }
        }
    }
    pub fn listen() -> std::io::Result<()> {
        let printname = Self::SOCKET_NAME;
        let name = printname.to_ns_name::<GenericNamespaced>()?;
        let opts = ListenerOptions::new().name(name);

        let listener = match opts.create_sync() {
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                eprintln!(
                "Error: could not start server because the socket file is occupied. Please check if
                {printname} is in use by another process and try again."
            );
                return Err(e);
            }
            x => x?,
        };

        eprintln!("Server running at {printname}");

        let mut buffer = String::with_capacity(128);

        for conn in listener.incoming().filter_map(Self::handle_error) {
            let mut conn = BufReader::new(conn);
            println!("Incoming connection!");

            conn.read_line(&mut buffer)?;

            conn.get_mut().write_all(b"Hello from server!\n")?;

            print!("Client answered: {buffer}");

            buffer.clear();
        }
        Ok(())
    }
}
fn main() -> std::io::Result<()> {
    Listener::listen()
}
