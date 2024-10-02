mod cli;
mod dispatcher;
mod display;
mod ipc;
mod justfile;
mod runner;

pub use cli::*;
pub use dispatcher::*;
pub use display::*;
pub use ipc::*;
pub use justfile::*;
pub use runner::*;

pub const SOCKET_NAME: &str = "shell-compose.sock";
