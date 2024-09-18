mod cli;
mod errors;
mod ipc;
mod spawner;

pub use cli::*;
pub use errors::*;
pub use ipc::*;
pub use spawner::*;

pub const SOCKET_NAME: &str = "process-dispatcher.sock";
