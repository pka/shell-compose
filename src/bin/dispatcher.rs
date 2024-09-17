use clap::Parser;
use process_dispatcher::*;
use std::env;
use std::process::{self, Child};
use std::time::Duration;

pub struct DispatcherProc {
    pub proc: Child,
}

impl DispatcherProc {
    pub fn new() -> DispatcherProc {
        let exe = env::current_exe().unwrap();
        DispatcherProc {
            proc: process::Command::new(exe)
                .arg("serve")
                // .stdin(Stdio::piped())
                // .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        }
    }
}

// impl Drop for DispatcherProc {
//     fn drop(&mut self) {
//         self.proc.kill().unwrap();
//     }
// }

fn cli() -> Result<(), IpcClientError> {
    let cli = Cli::parse();

    // if ipc_client_connect(SOCKET_NAME).is_err() {
    if send_ipc_message(SOCKET_NAME, &Message::NoCommand).is_err() {
        eprintln!("Starting dispatcher");
        let _ = DispatcherProc::new();
        std::thread::sleep(Duration::from_secs(1));
    }

    let response: Message = send_ipc_query(SOCKET_NAME, &Message::Ping)?;
    match response {
        Message::Ok => {}
        _ => Err(IpcClientError::PingError)?,
    }

    let msg: Message = cli.into();
    let response: Message = send_ipc_query(SOCKET_NAME, &msg)?;
    match response {
        Message::Ok => Ok(()),
        _ => Err(IpcClientError::CommandError),
    }
}

fn run_server() {
    start_ipc_server(
        SOCKET_NAME,
        |message: Message| match message {
            Message::NoCommand => None,
            Message::Ping => Some(Message::Ok),
            Message::Command(cmd) => Some(exec_command(cmd)),
            m => {
                dbg!(m);
                None
            }
        },
        Some(|e| panic!("Incoming connection error: {e}")),
    )
    .expect("Failed to start ipc listener")
    .join()
    .expect("Failed to join server thread");
}

fn exec_command(cmd: Command) -> Message {
    dbg!(&cmd);
    Message::Ok
}

fn main() -> Result<(), IpcClientError> {
    let args = std::env::args().collect::<Vec<_>>();

    if let Some("serve") = args.get(1).map(|s| s.as_str()) {
        run_server();
        Ok(())
    } else {
        cli()
    }
}
