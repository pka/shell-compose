mod errors;
pub use errors::*;

mod ipc;
pub use ipc::*;

mod cli;
pub use cli::*;

pub const SOCKET_NAME: &'static str = "process-dispatcher.sock";
