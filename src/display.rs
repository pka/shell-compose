use crate::{ProcInfo, ProcStatus};
use anstyle_query::{term_supports_ansi_color, truecolor};
use chrono::Local;
use comfy_table::{presets::UTF8_FULL, ContentArrangement, Table};
use env_logger::{
    fmt::style::{AnsiColor, Color, RgbColor, Style},
    Env,
};
use std::io::Write;

pub fn init_cli_logger() {
    let color = Formatter::default().log_color_app();
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    builder.format(move |buf, record| {
        let target = record.target();
        let time = buf.timestamp();
        // let level = record.level();

        writeln!(buf, "{color}{time} [{target}] {}{color:#}", record.args(),)
    });

    builder.init();
}

pub fn init_daemon_logger() {
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    builder.format(|buf, record| {
        let target = record.target();
        writeln!(buf, "[{target}] {}", record.args(),)
    });

    builder.init();
}

// See https://jvns.ca/blog/2024/10/01/terminal-colours/ for infos about color support

const PALETTE: [Style; 20] = [
    Style::new().fg_color(Some(Color::Rgb(RgbColor(0, 238, 110)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(11, 123, 224)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(2, 219, 129)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(3, 206, 142)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(9, 149, 198)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(7, 168, 179)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(4, 193, 154)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(8, 155, 192)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(5, 187, 161)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(6, 181, 167)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(12, 117, 230)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(6, 174, 173)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(1, 232, 116)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(8, 162, 186)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(4, 200, 148)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(9, 142, 205)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(10, 136, 211)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(3, 213, 135)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(1, 225, 123)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(11, 130, 217)))),
];

const ERR_PALETTE: [Style; 20] = [
    Style::new().fg_color(Some(Color::Rgb(RgbColor(237, 227, 66)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(251, 112, 199)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(249, 127, 182)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(253, 96, 217)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(250, 119, 191)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(240, 204, 93)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(241, 196, 102)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(242, 189, 110)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(239, 212, 84)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(243, 181, 119)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(244, 173, 128)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(245, 166, 137)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(238, 219, 75)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(246, 150, 155)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(254, 89, 226)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(247, 142, 164)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(248, 135, 173)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(246, 158, 146)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(252, 104, 208)))),
    Style::new().fg_color(Some(Color::Rgb(RgbColor(255, 81, 235)))),
];

const UNSTYLED: Style = Style::new();

pub struct Formatter {
    supports_truecolor: bool,
    supports_ansi_color: bool,
}

impl Default for Formatter {
    fn default() -> Self {
        Formatter {
            supports_truecolor: truecolor(),
            supports_ansi_color: term_supports_ansi_color(),
        }
    }
}

impl Formatter {
    pub fn log_color_proc(&self, idx: usize, err: bool) -> &'static Style {
        if self.supports_truecolor {
            if err {
                &ERR_PALETTE[idx % 20]
            } else {
                &PALETTE[idx % 20]
            }
        } else {
            &UNSTYLED
        }
    }

    pub fn log_color_app(&self) -> &'static Style {
        const COLOR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Magenta)));
        if self.supports_ansi_color {
            &COLOR
        } else {
            &UNSTYLED
        }
    }

    pub fn log_info(&self, text: &str) {
        let color = self.log_color_app();
        let time = Local::now().format("%F %T%.3f");
        println!("{color}{time} [dispatcher] {text}{color:#}")
    }
}

pub fn proc_info_table(proc_infos: &[ProcInfo]) {
    const EMPTY: String = String::new();

    fn clip_str(text: &str, max_len: usize) -> String {
        if text.len() > max_len {
            format!("{}...", &text[..max_len.max(3) - 3])
        } else {
            text.to_string()
        }
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_header(vec!["PID", "Status", "Command", "Start", "End"])
        .set_content_arrangement(ContentArrangement::DynamicFullWidth)
        .add_rows(proc_infos.iter().map(|info| {
            let status = match &info.state {
                ProcStatus::ExitOk => "Success".to_string(),
                ProcStatus::ExitErr(code) => format!("Error {code}"),
                ProcStatus::Unknown(err) => clip_str(err, 20),
                st => format!("{st:?}"),
            };
            let end = if let Some(ts) = info.end {
                format!("{}", ts.format("%F %T"))
            } else {
                EMPTY
            };
            vec![
                format!("{}", info.pid),
                status,
                clip_str(&info.command, 30),
                format!("{}", info.start.format("%F %T")),
                end,
            ]
        }));

    println!("{table}");
}
