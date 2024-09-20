use env_logger::{
    fmt::style::{AnsiColor, RgbColor, Style},
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

const PALETTE: [RgbColor; 20] = [
    RgbColor(0, 238, 110),
    RgbColor(11, 123, 224),
    RgbColor(2, 219, 129),
    RgbColor(3, 206, 142),
    RgbColor(9, 149, 198),
    RgbColor(7, 168, 179),
    RgbColor(4, 193, 154),
    RgbColor(8, 155, 192),
    RgbColor(5, 187, 161),
    RgbColor(6, 181, 167),
    RgbColor(12, 117, 230),
    RgbColor(6, 174, 173),
    RgbColor(1, 232, 116),
    RgbColor(8, 162, 186),
    RgbColor(4, 200, 148),
    RgbColor(9, 142, 205),
    RgbColor(10, 136, 211),
    RgbColor(3, 213, 135),
    RgbColor(1, 225, 123),
    RgbColor(11, 130, 217),
];

const ERR_PALETTE: [RgbColor; 20] = [
    RgbColor(237, 227, 66),
    RgbColor(251, 112, 199),
    RgbColor(249, 127, 182),
    RgbColor(253, 96, 217),
    RgbColor(250, 119, 191),
    RgbColor(240, 204, 93),
    RgbColor(241, 196, 102),
    RgbColor(242, 189, 110),
    RgbColor(239, 212, 84),
    RgbColor(243, 181, 119),
    RgbColor(244, 173, 128),
    RgbColor(245, 166, 137),
    RgbColor(238, 219, 75),
    RgbColor(246, 150, 155),
    RgbColor(254, 89, 226),
    RgbColor(247, 142, 164),
    RgbColor(248, 135, 173),
    RgbColor(246, 158, 146),
    RgbColor(252, 104, 208),
    RgbColor(255, 81, 235),
];

pub fn log_color(idx: usize, err: bool) -> Style {
    let col = if err {
        ERR_PALETTE[idx % 20]
    } else {
        PALETTE[idx % 20]
    };
    Style::new().fg_color(Some(col.into()))
}
