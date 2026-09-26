use anyhow::Context;
use chrono::Local;
use clap::Parser;
use gshocksrv::{
    bluetooth::BtleplugBackend,
    cli::Options,
    server::{Config, Server},
};
use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn main() -> anyhow::Result<()> {
    let options = Options::parse();
    init_logger(options.log_level, options.no_color);
    let stopped = Arc::new(AtomicBool::new(false));
    {
        let signal_stopped = Arc::clone(&stopped);
        ctrlc::set_handler(move || signal_stopped.store(true, Ordering::Relaxed)).context("install Ctrl+C handler")?;
    }
    let config = Config {
        fine_adjustment_secs: options.fine_adjustment_secs,
        scan_timeout: options.scan_timeout,
        request_timeout: options.request_timeout,
        store_path: options.store_path,
    };
    let mut server = Server::new(config, BtleplugBackend::with_stop(Arc::clone(&stopped)).context("initialize Bluetooth")?);
    {
        server.run(|| stopped.load(Ordering::Relaxed));
    }
    Ok(())
}

fn init_logger(log_level: log::LevelFilter, no_color: bool) {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log_level);
    builder.filter_module("btleplug::corebluetooth::peripheral", log::LevelFilter::Warn);
    builder.format(|buf, record| {
        let level_style = buf.default_level_style(record.level());
        writeln!(
            buf,
            "[{} {level_style}{}{level_style:#} {}] {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.target(),
            record.args()
        )
    });
    if no_color {
        builder.write_style(env_logger::WriteStyle::Never);
    }
    builder.init();
}
