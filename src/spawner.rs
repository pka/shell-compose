use crate::DispatcherError;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

struct ChildProc {
    proc: Child,
}

impl ChildProc {
    fn spawn(args: &[String]) -> Result<ChildProc, DispatcherError> {
        let mut cmd = VecDeque::from(args.to_owned());
        let Some(exe) = cmd.pop_front() else {
            return Err(DispatcherError::InvalidCommandError);
        };
        println!("Spawning {exe} {cmd:?}");
        let child = ChildProc {
            proc: Command::new(exe)
                .args(cmd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(DispatcherError::ProcSpawnError)?,
        };
        Ok(child)
    }
    fn is_running(&mut self) -> bool {
        matches!(self.proc.try_wait(), Ok(None))
    }
}

impl Drop for ChildProc {
    fn drop(&mut self) {
        self.proc.kill().unwrap();
    }
}

#[derive(Default)]
pub struct Spawner {
    procs: Vec<ChildProc>,
}

impl Spawner {
    pub fn new() -> Self {
        Spawner::default()
    }
    pub fn run(&mut self, args: &[String]) -> Result<(), DispatcherError> {
        let child = ChildProc::spawn(args)?;
        self.procs.push(child);
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
    pub fn log(&mut self) -> Result<(), DispatcherError> {
        // TODO: all processes or a specific one
        if let Some(child) = self.procs.get_mut(0) {
            if !child.is_running() {
                return Ok(());
            }
            let stdout = child.proc.stdout.take().unwrap();
            let stdout_reader = BufReader::new(stdout);
            let stderr = child.proc.stderr.take().unwrap();
            let stderr_reader = BufReader::new(stderr);
            stdout_reader
                .lines()
                .chain(stderr_reader.lines()) // FIXME: Appends after stdout
                .filter_map(|line| line.ok())
                .for_each(|line| println!("{}", line));
        }
        Ok(())
    }
}
