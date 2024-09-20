mod cli;
mod display;
mod errors;
mod ipc;
mod spawner;

pub use cli::*;
pub use display::*;
pub use errors::*;
pub use ipc::*;
pub use spawner::*;

pub const SOCKET_NAME: &str = "process-dispatcher.sock";
