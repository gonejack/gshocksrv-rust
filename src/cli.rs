use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Parser)]
#[command(name = "gshocksrv", version, about = "Synchronize Casio G-Shock watches over Bluetooth.")]
pub struct Options {
    /// Seconds added to watch time (-10..10).
    #[arg(long, default_value_t = 0, value_name = "SECONDS")]
    pub fine_adjustment_secs: i64,
    /// Maximum time for each Bluetooth scan.
    #[arg(long, default_value = "60s", value_name = "DURATION")]
    pub scan_timeout: DurationValue,
    /// Maximum time to wait for a watch response.
    #[arg(long, default_value = "5s", value_name = "DURATION")]
    pub request_timeout: DurationValue,
    /// Path to the state file.
    #[arg(long, default_value = "gshock_server_data.json")]
    pub store_path: PathBuf,
    /// Log level: trace, debug, info, warn, or error.
    #[arg(long, default_value = "info")]
    pub log_level: String,
    /// Disable colored log output.
    #[arg(long)]
    pub no_color: bool,
}

#[derive(Debug, Clone)]
pub struct DurationValue(pub Duration);

impl std::str::FromStr for DurationValue {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
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
        Ok(Self(Duration::from_secs(seconds)))
    }
}

impl Options {
    pub fn validate(&self) -> Result<(), String> {
        if !(-10..=10).contains(&self.fine_adjustment_secs) {
            return Err("--fine-adjustment-secs must be between -10 and 10".into());
        }
        if !matches!(self.log_level.to_ascii_lowercase().as_str(), "trace" | "debug" | "info" | "warn" | "warning" | "error") {
            return Err(format!("invalid --log-level {:?}", self.log_level));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn validates_adjustment() {
        let o = Options::try_parse_from(["gshocksrv", "--fine-adjustment-secs", "11"]).unwrap();
        assert!(o.validate().is_err());
    }
    #[test]
    fn parses_duration() {
        let o = Options::try_parse_from(["gshocksrv", "--scan-timeout", "2m"]).unwrap();
        assert_eq!(o.scan_timeout.0, Duration::from_secs(120));
    }
}
