use crate::DispatcherError;
use std::collections::VecDeque;
use std::process::{Child, Command};

struct ChildProc {
    proc: Child,
}

impl ChildProc {
    fn spawn(args: &Vec<String>) -> Result<ChildProc, DispatcherError> {
        let mut cmd = VecDeque::from(args.clone());
        let Some(exe) = cmd.pop_front() else {
            return Err(DispatcherError::InvalidCommandError);
        };
        println!("Spawning {exe} {cmd:?}");
        let child = ChildProc {
            proc: Command::new(exe)
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

pub struct Spawner {
    procs: Vec<ChildProc>,
}

impl Spawner {
    pub fn new() -> Self {
        Spawner { procs: Vec::new() }
    }
    pub fn run(&mut self, args: &Vec<String>) -> Result<(), DispatcherError> {
        let child = ChildProc::spawn(args)?;
        self.procs.push(child);
        // Wait for output
        std::thread::sleep(std::time::Duration::from_millis(500));
        Ok(())
    }
    pub fn ps(&mut self) -> Result<(), DispatcherError> {
        for child in &mut self.procs {
            let state = match child.proc.try_wait() {
                Ok(Some(status)) => format!("Exited with {status}"),
                Ok(None) => "Running".to_string(),
                Err(e) => format!("Error {e}"),
            };
            println!("PID: {} - {state}", child.proc.id());
        }
        Ok(())
    }
}
