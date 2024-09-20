use env_logger::{
    fmt::style::{AnsiColor, Style},
    Env,
};
use std::io::Write;

pub fn init_logger() {
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    builder.format(|buf, record| {
        let target = record.target();
        let time = buf.timestamp();
        // let level = record.level();
        let color = Style::new().fg_color(Some(AnsiColor::Magenta.into()));

        writeln!(buf, "{color}{time} [{target}] {}{color:#}", record.args(),)
    });

    builder.init();
}

pub fn log_color() -> Style {
    Style::new().fg_color(Some(AnsiColor::Magenta.into()))
}
