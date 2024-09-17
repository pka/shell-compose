mod errors;
pub use errors::*;

mod ipc;
pub use ipc::*;

use serde::{Deserialize, Serialize};

pub const SOCKET_NAME: &'static str = "process-dispatcher.sock";

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Hello,
    Text { text: String },
    Ping,
    Pong,
}
