use process_dispatcher::*;

fn main() {
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
