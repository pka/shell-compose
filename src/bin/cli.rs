use process_dispatcher::*;
use std::env;
use std::process::{Child, Command};
use std::time::Duration;

pub struct DispatcherProc {
    pub proc: Child,
}

impl DispatcherProc {
    pub fn new() -> DispatcherProc {
        let output_dir = env::current_exe().unwrap();
        let exe = output_dir.parent().unwrap().join("dispatcher");
        DispatcherProc {
            proc: Command::new(exe)
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

fn main() {
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
