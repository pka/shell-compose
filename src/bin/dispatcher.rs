use clap::Parser;
use env_logger::{
    fmt::style::{AnsiColor, Style},
    Env,
};
use log::info;
use process_dispatcher::*;
use std::io::Write;
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
        while send_ipc_message(SOCKET_NAME, &Message::NoCommand).is_err() {
            if wait_ms >= max_ms {
                return Err(DispatcherError::ProcSpawnTimeoutError);
            }
            thread::sleep(Duration::from_millis(50));
            wait_ms += 50;
        }
        Ok(())
    }
}

fn init_logger() {
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    builder.format(|buf, record| {
        let target = record.target();
        let time = buf.timestamp();
        // let level = record.level();
        let color = Style::new().fg_color(Some(AnsiColor::Magenta.into()));

        writeln!(buf, "{color}{time} [{target}] {}{color:#}", record.args(),)
    });

    builder.init();
}

fn cli() -> Result<(), DispatcherError> {
    let cli = Cli::parse();
    init_logger();

    // if ipc_client_connect(SOCKET_NAME).is_err() {
    if send_ipc_message(SOCKET_NAME, &Message::NoCommand).is_err() {
        info!("Starting dispatcher");
        let dispatcher = DispatcherProc::spawn();
        dispatcher.wait(2000)?;
    }

    info!("Sending command");
    let msg: Message = cli.into();
    let response: Message = send_ipc_query(SOCKET_NAME, &msg)?;
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
    start_ipc_server(
        SOCKET_NAME,
        move |message: Message| match message {
            Message::NoCommand => None,
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
        let _res = match cmd {
            Command::Run { args } => self.spawner.run(&args),
            Command::Ps => self.spawner.ps(),
            Command::Logs => self.spawner.log(),
        };
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
