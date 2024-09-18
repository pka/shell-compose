use clap::Parser;
use process_dispatcher::*;
use std::env;
use std::process::{self, Child};
use std::time::Duration;

struct DispatcherProc {
    _proc: Child,
}

impl DispatcherProc {
    fn spawn() -> DispatcherProc {
        let exe = env::current_exe().unwrap();
        DispatcherProc {
            _proc: process::Command::new(exe).arg("serve").spawn().unwrap(),
        }
    }
}

fn cli() -> Result<(), DispatcherError> {
    let cli = Cli::parse();

    // if ipc_client_connect(SOCKET_NAME).is_err() {
    if send_ipc_message(SOCKET_NAME, &Message::NoCommand).is_err() {
        eprintln!("Starting dispatcher");
        let _ = DispatcherProc::spawn();
        std::thread::sleep(Duration::from_secs(1));
    }

    let response: Message = send_ipc_query(SOCKET_NAME, &Message::Ping)?;
    match response {
        Message::Ok => {}
        _ => Err(DispatcherError::PingError)?,
    }

    let msg: Message = cli.into();
    let response: Message = send_ipc_query(SOCKET_NAME, &msg)?;
    match response {
        Message::Ok => Ok(()),
        _ => Err(DispatcherError::CommandError),
    }
}

fn run_server() {
    let mut dispatcher = Dispatcher {
        spawner: Spawner::new(),
    };
    start_ipc_server(
        SOCKET_NAME,
        move |message: Message| match message {
            Message::NoCommand => None,
            Message::Ping => Some(Message::Ok),
            Message::Command(cmd) => Some(dispatcher.exec_command(cmd)),
            m => {
                dbg!(m);
                None
            }
        },
        Some(|e| panic!("Incoming connection error: {e}")),
    )
    .expect("Failed to start ipc listener");
}

struct Dispatcher {
    spawner: Spawner,
}

impl Dispatcher {
    fn exec_command(&mut self, cmd: Command) -> Message {
        match cmd {
            Command::Run { args } => {
                self.spawner.run(&args).unwrap();
                Message::Ok
            }
            Command::Ps => {
                self.spawner.ps().unwrap();
                Message::Ok
            }
        }
    }
}

fn main() -> Result<(), DispatcherError> {
    let args = std::env::args().collect::<Vec<_>>();

    if let Some("serve") = args.get(1).map(|s| s.as_str()) {
        run_server();
        Ok(())
    } else {
        cli()
    }
}
