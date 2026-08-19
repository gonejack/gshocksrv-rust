use clap::Parser;
use gshocksrv::{
    bluetooth::BtleplugBackend,
    cli::Options,
    server::{Config, Server},
};

fn main() {
    let options = Options::parse();
    if let Err(e) = options.validate() {
        eprintln!("error: {e}");
        std::process::exit(2);
    }

    let mut builder = env_logger::Builder::new();
    builder.parse_filters(&options.log_level);
    if options.no_color {
        builder.write_style(env_logger::WriteStyle::Never);
    }
    builder.init();

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
    if let Err(e) = server.run(|| false) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
