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
        let mut proc = process::Command::new(exe);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            // CREATE_NO_WINDOW causes all children to not show a visible console window,
            // but it also apparently has the effect of starting a new process group.
            //
            // https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags#flags
            // https://stackoverflow.com/a/71364777/9423933
            proc.creation_flags(CREATE_NO_WINDOW);

            // See https://stackoverflow.com/a/78989930 for a possible alternative.
        }
        if env::var("RUST_LOG").unwrap_or("".to_string()) == "debug" {
            proc.env("RUST_LOG", "debug")
        } else {
            proc.stdout(Stdio::null()).stderr(Stdio::null())
        };
        proc.spawn().unwrap();
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
    let cli = CliCommand::augment_subcommands(cli);
    let mut cli = cli.about(env!("CARGO_PKG_DESCRIPTION")); // Overwritten by augment_subcommands
    let matches = cli.clone().get_matches();
    let exec_command = ExecCommand::from_arg_matches(&matches);
    let cli_command = CliCommand::from_arg_matches(&matches);
    if exec_command.is_err() && cli_command.is_err() {
        cli.print_help().ok();
        return Ok(());
    }

    init_cli_logger();

    if IpcStream::check_connection(SOCKET_NAME).is_err() {
        if matches!(cli_command, Ok(CliCommand::Exit)) {
            // Background process already exited
            return Ok(());
        }
        info!(target: "dispatcher", "Starting background process");
        let dispatcher = DispatcherProc::spawn();
        dispatcher.wait(2000)?;
    }

    let mut stream = IpcStream::connect("cli", SOCKET_NAME)?;
    let msg: Message = exec_command
        .map(Into::into)
        .or_else(|_| cli_command.map(Into::into))?;
    stream.send_message(&msg)?;
    if matches!(msg, Message::CliCommand(CliCommand::Exit)) {
        return Ok(());
    }
    let formatter = Formatter::default();
    let mut proc_infos = Vec::new();
    let mut job_infos = Vec::new();
    loop {
        let response = stream.receive_message();
        match response {
            Ok(Message::Connect) => {}
            Ok(Message::Ok) => {
                match msg {
                    Message::ExecCommand(_) | Message::CliCommand(CliCommand::Stop { .. }) => {
                        info!(target: "dispatcher", "Command successful");
                    }
                    Message::CliCommand(CliCommand::Ps) => {
                        proc_info_table(&proc_infos);
                    }
                    Message::CliCommand(CliCommand::Jobs) => {
                        job_info_table(&job_infos);
                    }
                    _ => {}
                }
                return Ok(());
            }
            Ok(Message::JobsStarted(job_ids)) => {
                match job_ids.len() {
                    0 => error!(target: "dispatcher", "No jobs started (services running)"),
                    1 => {
                        info!(target: "dispatcher", "Job {} started", job_ids.first().unwrap_or(&0))
                    }
                    _ => {
                        info!(target: "dispatcher", "Jobs {} started", job_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", "))
                    }
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
            Ok(Message::JobInfo(info)) => {
                job_infos.push(info);
            }
            Ok(Message::LogLine(log_line)) => {
                log_line.log(&formatter);
            }
            Err(e) => return Err(e.into()),
            _ => return Err(DispatcherError::UnexpectedMessageError),
        }
    }
}

fn main() -> Result<(), DispatcherError> {
    cli()
}
