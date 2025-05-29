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
        let cwd = std::env::current_dir().unwrap_or(".".into());
        dispatcher.exec_command(cmd, cwd);
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
            let Ok(Message::Connect) = stream.receive_message() else {
                error!("Connect message expected - closing IPC stream");
                return;
            };

            // loop { TODO: support multiple messages from same connection
            let Ok(request) = stream.receive_message() else {
                error!("Closing IPC stream after receive_message error");
                return;
            };
            match request {
                Message::Connect => {}
                Message::ExecCommand(cmd, cwd) => {
                    let response = dispatcher.exec_command(cmd, cwd);
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
