use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Parser)]
#[command(name = "gshocksrv", version, about = "Synchronize Casio G-Shock watches over Bluetooth.")]
pub struct Options {
    /// Seconds added to watch time (-10..10).
    #[arg(long, default_value_t = 0, value_name = "SECONDS", allow_negative_numbers = true, value_parser = clap::value_parser!(i64).range(-10..=10))]
    pub fine_adjustment_secs: i64,
    /// Maximum time for each Bluetooth scan.
    #[arg(long, default_value = "60s", value_name = "DURATION", value_parser = parse_duration)]
    pub scan_timeout: Duration,
    /// Maximum time to wait for a watch response.
    #[arg(long, default_value = "5s", value_name = "DURATION", value_parser = parse_duration)]
    pub request_timeout: Duration,
    /// Path to the state file.
    #[arg(long, default_value = "gshock_server_data.json")]
    pub store_path: PathBuf,
    /// Log level: trace, debug, info, warn, or error.
    #[arg(long, default_value = "info", value_parser = parse_log_level)]
    pub log_level: log::LevelFilter,
    /// Disable colored log output.
    #[arg(long)]
    pub no_color: bool,
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    let value = value.trim();
    let (number, unit) = value.split_at(value.find(|c: char| !c.is_ascii_digit()).unwrap_or(value.len()));
    let number: u64 = number.parse().map_err(|_| format!("invalid duration {value:?}"))?;
    let seconds = match unit.to_ascii_lowercase().as_str() {
        "" | "s" | "sec" | "secs" => Some(number),
        "m" | "min" | "mins" => number.checked_mul(60),
        "h" | "hr" | "hrs" => number.checked_mul(3600),
        _ => None,
    }
    .ok_or_else(|| format!("invalid duration {value:?}"))?;

    if seconds == 0 {
        return Err("duration must be positive".into());
    }

    Ok(Duration::from_secs(seconds))
}

fn parse_log_level(value: &str) -> Result<log::LevelFilter, String> {
    match value.to_ascii_lowercase().as_str() {
        "trace" => Ok(log::LevelFilter::Trace),
        "debug" => Ok(log::LevelFilter::Debug),
        "info" => Ok(log::LevelFilter::Info),
        "warn" | "warning" => Ok(log::LevelFilter::Warn),
        "error" => Ok(log::LevelFilter::Error),
        _ => Err(format!("invalid --log-level {value:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn validates_adjustment() {
        for value in ["-11", "11"] {
            assert!(Options::try_parse_from(["gshocksrv", "--fine-adjustment-secs", value]).is_err());
        }
        assert_eq!(Options::try_parse_from(["gshocksrv", "--fine-adjustment-secs", "-10"]).unwrap().fine_adjustment_secs, -10);
    }

    #[test]
    fn parses_log_level() {
        assert_eq!(Options::try_parse_from(["gshocksrv", "--log-level", "WARNING"]).unwrap().log_level, log::LevelFilter::Warn);
        for value in ["off", "invalid"] {
            assert!(Options::try_parse_from(["gshocksrv", "--log-level", value]).is_err());
        }
    }

    #[test]
    fn parses_duration() {
        let options = Options::try_parse_from(["gshocksrv", "--scan-timeout", "2m"]).unwrap();
        assert_eq!(options.scan_timeout, Duration::from_secs(120));
    }
}
