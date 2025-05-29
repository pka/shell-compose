use crate::command::*;
use crate::dispatcher::Job;
use crate::display::*;
use crate::ipc::*;
use crate::runner::ProcInfo;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph, TableState},
    DefaultTerminal, Frame,
};
use std::time::{Duration, Instant};

pub fn run() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    if log::max_level() < log::LevelFilter::Debug {
        ratatui::restore();
    }
    result
}

/// The main application which holds the state and logic of the application.
pub struct App {
    stream: IpcStream,
    ps_infos: Vec<ProcInfo>,
    job_infos: Vec<Job>,
    windows: Vec<Window>,
    active_window: usize,
    /// Is the application running?
    running: bool,
}

impl App {
    /// Construct a new instance of [`App`].
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let stream = IpcStream::connect("tui").expect("IPC connection failed");
        let keys = "Scroll [↓↑] | Quit [Esc] or [q]) ";
        let mut windows = vec![
            Window::new(" Processes [1] ", keys),
            Window::new(" Jobs [2] ", keys),
            Window::new(" Log [3] ", keys),
        ];
        let active_window = 0;
        windows[active_window].set_active(true);

        // LogLine(LogLine),
        Self {
            stream,
            ps_infos: vec![],
            job_infos: vec![],
            windows,
            active_window,
            running: false,
        }
    }

    /// Run the application's main loop.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        self.running = true;
        let update_rate = Duration::from_millis(1000);
        let mut last_tick = Instant::now() - update_rate;
        while self.running {
            if last_tick.elapsed() >= update_rate {
                self.fetch_data()?;
                last_tick = Instant::now();
            }
            terminal.draw(|frame| self.render(frame))?;
            self.handle_crossterm_events()?;
        }
        Ok(())
    }

    fn fetch_data(&mut self) -> Result<(), IpcClientError> {
        self.stream = IpcStream::connect("tui")?; // TODO: reuse connection
        self.stream
            .send_message(&Message::CliCommand(CliCommand::Ps))?;
        if let Message::PsInfo(ps_infos) = self.stream.receive_message()? {
            self.ps_infos = ps_infos;
        }
        self.stream = IpcStream::connect("tui")?;
        self.stream
            .send_message(&Message::CliCommand(CliCommand::Jobs))?;
        if let Message::JobInfo(job_infos) = self.stream.receive_message()? {
            self.job_infos = job_infos;
        }
        Ok(())
    }

    fn window(&mut self) -> &mut Window {
        &mut self.windows[self.active_window]
    }

    fn set_active_window(&mut self, window_idx: usize) {
        self.window().set_active(false);
        self.active_window = window_idx;
        self.window().set_active(true);
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

        macro_rules! block_fn {
            ($window:expr, $area:expr, $col:ident) => {{
                $window.set_table_height($area.height);
                let title = Line::from($window.title.as_str()).bold().$col().centered();
                let mut block = Block::bordered()
                    .border_style($window.border_style())
                    .title(title);
                if $window.active {
                    block = block.title_bottom(Line::from($window.keys.as_str()).centered());
                }
                block
            }};
        }
        // top-left window
        let area = vertical_chunks[0];
        let window = &mut self.windows[0];
        let table = proc_info_ui_table(&self.ps_infos)
            .row_highlight_style(Style::new().reversed())
            .block(block_fn!(window, area, blue));
        frame.render_stateful_widget(table, area, &mut window.table_state);

        // bottom-left window
        let area = vertical_chunks[1];
        let window = &mut self.windows[1];
        let table = job_info_ui_table(&self.job_infos)
            .row_highlight_style(Style::new().reversed())
            .block(block_fn!(window, area, green));
        frame.render_stateful_widget(table, area, &mut window.table_state);

        // right window
        let area = horizontal_chunks[1];
        let window = &mut self.windows[2];
        let widget = Paragraph::new("This is the log window content.")
            .block(block_fn!(window, area, red))
            .centered();
        frame.render_widget(widget, area);
    }

    /// Reads the crossterm events and updates the state of [`App`].
    ///
    /// If your application needs to perform work in between handling events, you can use the
    /// [`event::poll`] function to check if there are any events available with a timeout.
    fn handle_crossterm_events(&mut self) -> color_eyre::Result<()> {
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
            (_, KeyCode::Char('1')) => self.set_active_window(0),
            (_, KeyCode::Char('2')) => self.set_active_window(1),
            (_, KeyCode::Char('3')) => self.set_active_window(2),
            (_, KeyCode::Tab) => {
                self.set_active_window((self.active_window + 1) % self.windows.len())
            }
            (_, KeyCode::BackTab) => self.set_active_window(
                (self.active_window + self.windows.len() - 1) % self.windows.len(),
            ),
            (_, KeyCode::Up) => self.window().up(),
            (_, KeyCode::Down) => self.window().down(),
            (_, KeyCode::PageUp) => self.window().page_up(),
            (_, KeyCode::PageDown) => self.window().page_down(),
            _ => {}
        }
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

struct Window {
    title: String,
    keys: String,
    active: bool,
    selected_row: Option<usize>,
    table_state: TableState,
    table_height: u16,
}

impl Window {
    pub fn new(title: &str, keys: &str) -> Self {
        Window {
            title: title.to_string(),
            keys: keys.to_string(),
            active: false,
            selected_row: None,
            table_state: TableState::new(),
            table_height: 0,
        }
    }
    fn set_table_height(&mut self, height: u16) {
        self.table_height = height;
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
        if self.active {
            if self.selected_row.is_none() {
                self.table_state.select_first();
            } else {
                self.table_state.select(self.selected_row);
            }
        } else {
            self.selected_row = self.table_state.selected();
            self.table_state.select(None);
        }
    }
    fn up(&mut self) {
        self.table_state.select_previous();
    }
    fn down(&mut self) {
        self.table_state.select_next();
    }
    fn page_up(&mut self) {
        self.table_state.scroll_up_by(self.table_height);
    }
    fn page_down(&mut self) {
        self.table_state.scroll_down_by(self.table_height);
    }
    fn border_style(&self) -> Style {
        let active_window_border = Style::new().yellow();
        let inactive_window_border = Style::new();

        if self.active {
            active_window_border
        } else {
            inactive_window_border
        }
    }
}
