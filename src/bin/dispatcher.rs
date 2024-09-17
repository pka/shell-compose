use process_dispatcher::*;
use std::env;
use std::process::{Child, Command};
use std::time::Duration;

pub struct DispatcherProc {
    pub proc: Child,
}

impl DispatcherProc {
    pub fn new() -> DispatcherProc {
        let exe = env::current_exe().unwrap();
        DispatcherProc {
            proc: Command::new(exe)
                .arg("serve")
                // .stdin(Stdio::piped())
                // .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        }
    }
}

// impl Drop for DispatcherProc {
//     fn drop(&mut self) {
//         self.proc.kill().unwrap();
//     }
// }

fn cli() {
    // if ipc_client_connect(SOCKET_NAME).is_err() {
    if send_ipc_message(SOCKET_NAME, &Message::Hello).is_err() {
        eprintln!("Starting dispatcher");
        let _ = DispatcherProc::new();
        std::thread::sleep(Duration::from_secs(1));
    }

    let text = Message::Text {
        text: "Hello from client!".to_string(),
    };

    let ping = Message::Ping;

    send_ipc_message(SOCKET_NAME, &text).expect("Failed to connect to socket");

    let response: Message =
        send_ipc_query(SOCKET_NAME, &ping).expect("Failed to connect to socket");

    dbg!(response);
}

fn run_server() {
    start_ipc_server(
        SOCKET_NAME,
        |message: Message| match message {
            Message::Text { text } => {
                println!("{text}");
                None
            }
            Message::Ping => Some(Message::Pong),
            _ => None,
        },
        Some(|e| panic!("Incoming connection error: {e}")),
    )
    .expect("Failed to start ipc listener")
    .join()
    .expect("Failed to join server thread");
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    if let Some("serve") = args.get(1).map(|s| s.as_str()) {
        run_server()
    } else {
        cli()
    }
}
