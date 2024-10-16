use clap::{CommandFactory, FromArgMatches, Subcommand};
use log::error;
use shell_compose::{
    init_daemon_logger, start_ipc_listener, Cli, Dispatcher, ExecCommand, IpcStream, Message,
};
use std::fs::remove_file;

fn run_server() {
    let cli = Cli::command();
    let cli = ExecCommand::augment_subcommands(cli);
    let cli = cli.about(env!("CARGO_PKG_DESCRIPTION")); // Overwritten by augment_subcommands
    let matches = cli.clone().get_matches();
    let exec_command = ExecCommand::from_arg_matches(&matches);

    init_daemon_logger();

    let mut dispatcher = Dispatcher::create();

    // Execute commands from CLI
    if let Ok(cmd) = exec_command {
        dispatcher.exec_command(cmd);
    }

    let socket_name = IpcStream::user_socket_name();
    // reclaim_name in interprocess::local_socket::ListenerOptions
    // does not work, so we delete the socket first.
    if IpcStream::check_connection().is_err() {
        remove_file(&socket_name).ok();
    }
    start_ipc_listener(
        &socket_name,
        move |mut stream| {
            let Ok(_connect) = stream.receive_message() else {
                return;
            };

            let Ok(request) = stream.receive_message() else {
                return;
            };
            match request {
                Message::Connect => {}
                Message::ExecCommand(cmd) => {
                    let response = dispatcher.exec_command(cmd);
                    stream.send_message(&response).unwrap()
                }
                Message::CliCommand(cmd) => dispatcher.cli_command(cmd, &mut stream),
                msg => {
                    error!("Unexpected protocol message: `{msg:?}`");
                }
            }
        },
        Some(|e| panic!("Incoming connection error: {e}")),
    )
    .expect("Failed to start ipc listener");
}

fn main() {
    run_server();
}
