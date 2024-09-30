use clap::Parser;
use log::info;
use shell_compose::*;
use std::process::{self, Child};
use std::time::Duration;
use std::{env, thread};

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
    fn wait(&self, max_ms: u64) -> Result<(), DispatcherError> {
        let mut wait_ms = 0;
        while IpcStream::check_connection(SOCKET_NAME).is_err() {
            if wait_ms >= max_ms {
                return Err(DispatcherError::ProcSpawnTimeoutError);
            }
            thread::sleep(Duration::from_millis(50));
            wait_ms += 50;
        }
        Ok(())
    }
}

fn cli() -> Result<(), DispatcherError> {
    let cli = Cli::parse();
    init_logger();

    if IpcStream::check_connection(SOCKET_NAME).is_err() {
        info!(target: "dispatcher", "Starting dispatcher");
        let dispatcher = DispatcherProc::spawn();
        dispatcher.wait(2000)?;
    }

    info!(target: "dispatcher", "Sending command");
    let mut stream = IpcStream::connect(SOCKET_NAME)?;
    let msg: Message = cli.into();
    stream.send_message(&msg)?;
    let response: Message = stream.receive_message()?;
    match response {
        Message::Ok => Ok(()),
        _ => Err(DispatcherError::CommandError),
    }
}

fn run_server() {
    init_logger();
    let mut dispatcher = Dispatcher {
        spawner: Spawner::new(),
    };
    start_ipc_listener(
        SOCKET_NAME,
        move |mut stream| {
            let request = stream.receive_message().unwrap();
            if let Some(response) = match request {
                Message::NoCommand => None,
                Message::Command(cmd) => Some(dispatcher.exec_command(cmd)),
                m => {
                    dbg!(m);
                    None
                }
            } {
                stream.send_message(&response).unwrap();
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
        let res = match cmd {
            Command::Run { args } => self.spawner.run(&args),
            Command::Runat { at, args } => self.spawner.run_at(&at, &args),
            Command::Ps => self.spawner.ps(),
            Command::Logs => self.spawner.log(),
        };
        if let Err(e) = res {
            println!("{e}");
        }
        Message::Ok
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
