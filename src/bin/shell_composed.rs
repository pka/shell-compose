use clap::{CommandFactory, FromArgMatches, Subcommand};
use log::{error, info};
use shell_compose::*;

fn run_server() {
    let cli = Cli::command();
    let cli = ExecCommand::augment_subcommands(cli);
    let matches = cli.clone().get_matches();
    let exec_command = ExecCommand::from_arg_matches(&matches);

    init_daemon_logger();

    let mut dispatcher = Dispatcher::new();

    // Execute commands from CLI
    if let Ok(cmd) = exec_command {
        dispatcher.exec_command(cmd);
    }

    start_ipc_listener(
        SOCKET_NAME,
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
                Message::QueryCommand(cmd) => dispatcher.query_command(cmd, &mut stream),
                m => {
                    // Unexpected command
                    dbg!(m);
                }
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
    fn new() -> Self {
        Dispatcher {
            spawner: Spawner::new(),
        }
    }
    fn exec_command(&mut self, cmd: ExecCommand) -> Message {
        info!("Executing `{cmd:?}`");
        let res = match cmd {
            ExecCommand::Run { args } => self.spawner.run(&args),
            ExecCommand::Runat { at, args } => self.spawner.run_at(&at, &args),
            ExecCommand::Start { service } => self.spawner.start(&service),
            ExecCommand::Up { group } => self.spawner.up(&group),
        };
        if let Err(e) = &res {
            error!("{e}");
        }
        res.into()
    }
    fn query_command(&mut self, cmd: QueryCommand, stream: &mut IpcStream) {
        info!("Executing `{cmd:?}`");
        let res = match cmd {
            QueryCommand::Ps => self.spawner.ps(stream),
            QueryCommand::Logs => self.spawner.log(stream),
        };
        if let Err(e) = &res {
            error!("{e}");
        }
        let _ = stream.send_message(&res.into());
    }
}

fn main() {
    run_server();
}
