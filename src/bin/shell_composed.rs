use clap::{CommandFactory, FromArgMatches, Subcommand};
use shell_compose::*;

fn run_server() {
    let cli = Cli::command();
    let cli = ExecCommand::augment_subcommands(cli);
    let matches = cli.clone().get_matches();
    let exec_command = ExecCommand::from_arg_matches(&matches);

    init_logger();

    let mut dispatcher = Dispatcher {
        spawner: Spawner::new(),
    };

    // Execute commands from CLI
    if let Ok(cmd) = exec_command {
        dispatcher.exec_command(cmd);
    }

    start_ipc_listener(
        SOCKET_NAME,
        move |mut stream| {
            let request = stream.receive_message().unwrap();
            if let Some(response) = match request {
                Message::NoCommand => None,
                Message::ExecCommand(cmd) => Some(dispatcher.exec_command(cmd)),
                Message::QueryCommand(cmd) => Some(dispatcher.query_command(cmd)),
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
    fn exec_command(&mut self, cmd: ExecCommand) -> Message {
        let res = match cmd {
            ExecCommand::Run { args } => self.spawner.run(&args),
            ExecCommand::Runat { at, args } => self.spawner.run_at(&at, &args),
        };
        if let Err(e) = res {
            println!("{e}");
        }
        Message::Ok
    }
    fn query_command(&mut self, cmd: QueryCommand) -> Message {
        let res = match cmd {
            QueryCommand::Ps => self.spawner.ps(),
            QueryCommand::Logs => self.spawner.log(),
        };
        if let Err(e) = res {
            println!("{e}");
        }
        Message::Ok
    }
}

fn main() {
    run_server();
}
