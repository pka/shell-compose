use clap::{CommandFactory, FromArgMatches, Subcommand};
use log::{error, info};
use shell_compose::*;
use std::process::{self, Stdio};
use std::time::Duration;
use std::{env, thread};

struct DispatcherProc;

impl DispatcherProc {
    fn spawn() -> DispatcherProc {
        let mut exe = env::current_exe().unwrap();
        exe.set_file_name(
            exe.file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap()
                .replace("compose", "composed"),
        );
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
        // TODO: return if QueryCommand::Exit
        info!(target: "dispatcher", "Starting background process");
        let dispatcher = DispatcherProc::spawn();
        dispatcher.wait(2000)?;
    }

    let mut stream = IpcStream::connect("cli", SOCKET_NAME)?;
    let msg: Message = exec_command
        .map(Into::into)
        .or_else(|_| query_command.map(Into::into))?;
    stream.send_message(&msg)?;
    if matches!(msg, Message::QueryCommand(QueryCommand::Exit)) {
        return Ok(());
    }
    let mut proc_infos = Vec::new();
    loop {
        let response = stream.receive_message();
        match response {
            Ok(Message::Connect) => {}
            Ok(Message::Ok) => {
                match msg {
                    Message::ExecCommand(_) => {
                        info!(target: "dispatcher", "Command successful");
                    }
                    Message::QueryCommand(QueryCommand::Ps) => {
                        proc_info_table(&proc_infos);
                    }
                    _ => {}
                }
                return Ok(());
            }
            Ok(Message::Err(msg)) => {
                error!(target: "dispatcher", "{msg} - Check logs for more information");
                return Ok(());
            }
            Ok(Message::PsInfo(info)) => {
                proc_infos.push(info);
            }
            Ok(Message::LogLine(log_line)) => {
                log_line.log();
            }
            Err(e) => return Err(e.into()),
            _ => return Err(DispatcherError::UnexpectedMessageError),
        }
    }
}

fn main() -> Result<(), DispatcherError> {
    cli()
}
