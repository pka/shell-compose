use crate::command::*;
use crate::dispatcher::Job;
use crate::display::*;
use crate::ipc::*;
use crate::runner::ProcInfo;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::Stylize,
    text::Line,
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};

pub fn run() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    ratatui::restore();
    result
}

/// The main application which holds the state and logic of the application.
#[derive(Debug, Default)]
pub struct App {
    ps_infos: Vec<ProcInfo>,
    job_infos: Vec<Job>,
    /// Is the application running?
    running: bool,
}

impl App {
    /// Construct a new instance of [`App`].
    pub fn new() -> Self {
        let mut stream = IpcStream::connect("tui").unwrap();
        stream
            .send_message(&Message::CliCommand(CliCommand::Ps))
            .unwrap();
        let Ok(Message::PsInfo(ps_infos)) = stream.receive_message() else {
            eprintln!("Failed to receive process infos");
            std::process::exit(1);
        };
        let mut stream = IpcStream::connect("tui").unwrap();
        stream
            .send_message(&Message::CliCommand(CliCommand::Jobs))
            .unwrap();
        let Ok(Message::JobInfo(job_infos)) = stream.receive_message() else {
            eprintln!("Failed to receive job infos");
            std::process::exit(1);
        };
        // LogLine(LogLine),
        Self {
            ps_infos,
            job_infos,
            running: false,
        }
    }

    /// Run the application's main loop.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;
        while self.running {
            terminal.draw(|frame| self.render(frame))?;
            self.handle_crossterm_events()?;
        }
        Ok(())
    }

    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    ///
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/main/ratatui-widgets/examples>
    fn render(&mut self, frame: &mut Frame) {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(frame.area());

        // Split the left portion vertically
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(horizontal_chunks[0]);

        // top-left window
        let ps_title = Line::from("Processes [1]").bold().blue().centered();
        let keys = Line::from("Scroll [↓↑] | Quit [Esc] or [q]) ").centered();
        let table = proc_info_ui_table(&self.ps_infos)
            .block(Block::bordered().title(ps_title).title_bottom(keys));
        frame.render_widget(table, vertical_chunks[0]);

        // bottom-left window
        let bottom_title = Line::from("Jobs [2]").bold().green().centered();
        let table = job_info_ui_table(&self.job_infos).block(Block::bordered().title(bottom_title));
        frame.render_widget(table, vertical_chunks[1]);

        // right window
        let log_title = Line::from("Log [3]").bold().red().centered();
        frame.render_widget(
            Paragraph::new("This is the log window content.")
                .block(Block::bordered().title(log_title))
                .centered(),
            horizontal_chunks[1],
        );
    }

    /// Reads the crossterm events and updates the state of [`App`].
    ///
    /// If your application needs to perform work in between handling events, you can use the
    /// [`event::poll`] function to check if there are any events available with a timeout.
    fn handle_crossterm_events(&mut self) -> Result<()> {
        match event::read()? {
            // it's important to check KeyEventKind::Press to avoid handling key release events
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn on_key_event(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),
            // Add other key handlers here.
            _ => {}
        }
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}
