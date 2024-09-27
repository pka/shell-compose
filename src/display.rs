use chrono::{Local, SecondsFormat};
use env_logger::{
    fmt::style::{AnsiColor, Color, RgbColor, Style},
    Env,
};
use std::io::Write;

pub fn init_logger() {
    const COLOR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Magenta)));
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    builder.format(|buf, record| {
        let target = record.target();
        let time = buf.timestamp();
        // let level = record.level();

        writeln!(buf, "{COLOR}{time} [{target}] {}{COLOR:#}", record.args(),)
    });

    builder.init();
}

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

pub fn log_color(idx: usize, err: bool) -> &'static Style {
    if err {
        &ERR_PALETTE[idx % 20]
    } else {
        &PALETTE[idx % 20]
    }
}

pub fn log_info(text: &str) {
    const COLOR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Magenta)));

    let time = Local::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    println!("{COLOR}{time} [dispatcher] {text}{COLOR:#}")
}
