use clap::Parser;
use gshocksrv::{
    bluetooth::BtleplugBackend,
    cli::Options,
    server::{Config, Server},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn main() {
    let options = Options::parse();
    if let Err(e) = options.validate() {
        eprintln!("error: {e}");
        std::process::exit(2);
    }

    let mut builder = env_logger::Builder::new();
    builder.parse_filters(&options.log_level);
    builder.filter_module("btleplug::corebluetooth::peripheral", log::LevelFilter::Warn);
    if options.no_color {
        builder.write_style(env_logger::WriteStyle::Never);
    }
    builder.init();

    let stopped = Arc::new(AtomicBool::new(false));
    let signal_stopped = Arc::clone(&stopped);
    if let Err(e) = ctrlc::set_handler(move || signal_stopped.store(true, Ordering::Relaxed)) {
        eprintln!("error: could not install Ctrl+C handler: {e}");
        std::process::exit(1);
    }

    let config = Config {
        fine_adjustment_secs: options.fine_adjustment_secs,
        scan_timeout: options.scan_timeout,
        request_timeout: options.request_timeout,
        store_path: options.store_path,
    };
    let backend = match BtleplugBackend::new() {
        Ok(backend) => backend,
        Err(e) => {
            eprintln!("error: Bluetooth initialization failed: {e:#}");
            std::process::exit(1);
        }
    };
    let mut server = Server::new(config, backend);
    if let Err(e) = server.run(|| stopped.load(Ordering::Relaxed)) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
