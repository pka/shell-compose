use clap::{CommandFactory, FromArgMatches, Subcommand};
use log::info;
use shell_compose::*;
use std::process::{self, Stdio};
use std::time::Duration;
use std::{env, thread};

struct DispatcherProc;

impl DispatcherProc {
    fn spawn() -> DispatcherProc {
        let mut exe = env::current_exe().unwrap().into_os_string();
        exe.push("d");
        let _proc = process::Command::new(exe)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // .env("RUST_LOG", "debug")
            .spawn()
            .unwrap();
        DispatcherProc
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
    let cli = Cli::command();
    let cli = ExecCommand::augment_subcommands(cli);
    let cli = QueryCommand::augment_subcommands(cli);
    let matches = cli.get_matches();
    let exec_command = ExecCommand::from_arg_matches(&matches);
    let query_command = QueryCommand::from_arg_matches(&matches);

    init_cli_logger();

    if IpcStream::check_connection(SOCKET_NAME).is_err() {
        info!(target: "dispatcher", "Starting dispatcher");
        let dispatcher = DispatcherProc::spawn();
        dispatcher.wait(2000)?;
    }

    info!(target: "dispatcher", "Sending command");
    let mut stream = IpcStream::connect("cli", SOCKET_NAME)?;
    let msg: Message = exec_command
        .map(Into::into)
        .or_else(|_| query_command.map(Into::into))?;
    stream.send_message(&msg)?;
    loop {
        let response = stream.receive_message();
        match response {
            Ok(Message::Ok) => return Ok(()),
            Ok(Message::PsInfo(info)) => {
                println!("PID: {} - {}", info.pid, info.state);
                return Ok(());
            }
            Ok(Message::LogLine(log_line)) => {
                log_line.log();
            }
            Err(e) => return Err(e.into()),
            _ => return Err(DispatcherError::CommandError),
        }
    }
}

fn main() -> Result<(), DispatcherError> {
    cli()
}
