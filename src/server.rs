use crate::gshock::{self, Button};
use chrono::Local;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Config {
    pub fine_adjustment_secs: i64,
    pub scan_timeout: Duration,
    pub request_timeout: Duration,
    pub store_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("state file: {0}")]
    State(#[from] std::io::Error),
    #[error("state format: {0}")]
    Format(#[from] serde_json::Error),
    #[error("bluetooth: {0}")]
    Bluetooth(String),
}

pub trait ConnectedWatch {
    fn name(&self) -> &str;
    fn address(&self) -> &str;
    fn always_connected(&self) -> bool;
    fn pressed_button(&mut self, timeout: Duration) -> Result<Button, String>;
    fn set_time(&mut self, adjustment_secs: i64, timeout: Duration) -> Result<chrono::DateTime<Local>, String>;
    fn disconnect(&mut self) -> Result<(), String>;
}
pub trait BluetoothBackend {
    fn scan_and_connect(&mut self, timeout: Duration, accept: &dyn Fn(&str) -> bool) -> Result<Box<dyn ConnectedWatch>, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct State {
    pub last_connected: Option<String>,
    pub watch_name: Option<String>,
}
pub struct Store {
    path: PathBuf,
    pub data: State,
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ServerError> {
        let path = path.as_ref().to_path_buf();
        let data = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { path, data })
    }
    pub fn update(&mut self, data: State) -> Result<(), ServerError> {
        let mut bytes = serde_json::to_vec_pretty(&data)?;
        bytes.push(b'\n');
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &self.path)?;
        self.data = data;
        Ok(())
    }
}

pub struct ConnectionLimiter {
    last: Mutex<HashMap<String, Instant>>,
    interval: Duration,
}
impl ConnectionLimiter {
    pub fn new() -> Self {
        Self { last: Mutex::new(HashMap::new()), interval: Duration::from_secs(6 * 3600) }
    }
    pub fn allow(&self, name: &str) -> bool {
        if !gshock::is_always_connected(name) {
            return true;
        }
        let mut last = self.last.lock().expect("connection limiter mutex poisoned");
        let now = Instant::now();
        if last.get(name).is_some_and(|t| now.duration_since(*t) <= self.interval) {
            return false;
        }
        last.insert(name.to_owned(), now);
        true
    }
}
impl Default for ConnectionLimiter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Server<B> {
    pub config: Config,
    backend: B,
    limiter: ConnectionLimiter,
}
impl<B: BluetoothBackend> Server<B> {
    pub fn new(config: Config, backend: B) -> Self {
        Self { config, backend, limiter: ConnectionLimiter::new() }
    }
    pub fn run(&mut self, stop: impl Fn() -> bool) -> Result<(), ServerError> {
        let mut store = Store::open(&self.config.store_path).unwrap_or_else(|e| {
            warn!("state file could not be loaded: {e}");
            Store { path: self.config.store_path.clone(), data: State::default() }
        });
        info!("Long-press LOWER-LEFT or short-press LOWER-RIGHT on the watch to set time");
        while !stop() {
            thread::sleep(Duration::from_secs(1));
            info!("Waiting for connection...");
            let accept = |name: &str| name != "CASIO OCW-T200" && self.limiter.allow(name);
            let mut watch = match self.backend.scan_and_connect(self.config.scan_timeout, &accept) {
                Ok(w) => w,
                Err(e) => {
                    info!("failed to connect: {e}");
                    continue;
                }
            };
            info!("Connected watch={} address={}", watch.name(), watch.address());
            let state = State { last_connected: Some(Local::now().format("%m/%d %H:%M").to_string()), watch_name: Some(watch.name().to_string()) };
            if let Err(e) = store.update(state) {
                warn!("state file could not be saved: {e}");
            }
            match watch.pressed_button(self.config.request_timeout) {
                Ok(button) if should_set_time(button) => match watch.set_time(self.config.fine_adjustment_secs, self.config.request_timeout) {
                    Ok(t) => info!("Set time done watch={} time={} adjust={}", watch.name(), t.to_rfc3339(), self.config.fine_adjustment_secs),
                    Err(e) => error!("Set time failed watch={}: {e}", watch.name()),
                },
                Ok(_) => info!("connection ignored: unsupported button"),
                Err(e) => error!("button query failed: {e}"),
            }
            if !watch.always_connected() {
                watch.disconnect().unwrap_or_else(|e| warn!("disconnect failed: {e}"));
            }
        }
        info!("Server stopped");
        Ok(())
    }
}

fn should_set_time(button: Button) -> bool {
    matches!(button, Button::LowerLeft | Button::LowerRight | Button::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn store_round_trip() {
        let d = tempdir().unwrap();
        let p = d.path().join("state.json");
        let mut s = Store::open(&p).unwrap();
        let v = State { last_connected: Some("08/18 12:34".into()), watch_name: Some("CASIO GW-B5600".into()) };
        s.update(v.clone()).unwrap();
        assert_eq!(Store::open(p).unwrap().data, v);
    }
    #[test]
    fn limiter() {
        let l = ConnectionLimiter::new();
        assert!(l.allow("CASIO ECB-30"));
        assert!(!l.allow("CASIO ECB-30"));
        assert!(l.allow("CASIO GW-B5600"));
        assert!(l.allow("CASIO GW-B5600"));
    }
    #[test]
    fn automatic_connection_sets_time_without_a_button() {
        assert!(should_set_time(Button::None));
        assert!(should_set_time(Button::LowerLeft));
        assert!(should_set_time(Button::LowerRight));
        assert!(!should_set_time(Button::Invalid));
    }
}
