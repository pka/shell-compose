use shell_compose::*;

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

fn main() {
    run_server();
}
