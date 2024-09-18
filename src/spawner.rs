use crate::DispatcherError;
use std::collections::VecDeque;
use std::process::{Child, Command};

struct ChildProc {
    _proc: Child,
}

impl ChildProc {
    fn spawn(args: &Vec<String>) -> Result<ChildProc, DispatcherError> {
        let mut cmd = VecDeque::from(args.clone());
        let Some(exe) = cmd.pop_front() else {
            return Err(DispatcherError::InvalidCommandError);
        };
        println!("Spawning {exe} {cmd:?}");
        let child = ChildProc {
            _proc: Command::new(exe)
                .args(cmd)
                // .stdout(Stdio::piped())
                // .stderr(Stdio::piped())
                .spawn()
                .map_err(DispatcherError::ProcSpawnError)?,
        };
        Ok(child)
    }
}

// impl Drop for ChildProc {
//     fn drop(&mut self) {
//         self.proc.kill().unwrap();
//     }
// }

pub struct Spawner {}

impl Spawner {
    pub fn new() -> Self {
        Spawner {}
    }
    pub fn run(&self, args: &Vec<String>) -> Result<(), DispatcherError> {
        let _child = ChildProc::spawn(args)?;
        // Wait for output
        std::thread::sleep(std::time::Duration::from_millis(500));
        Ok(())
    }
}
