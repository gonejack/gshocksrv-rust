use crate::gshock::{self, Button};
use anyhow::{Error as AnyhowError, Result};
use chrono::Local;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
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
}

pub trait ConnectedWatch {
    fn name(&self) -> &str;
    fn address(&self) -> &str;
    fn always_connected(&self) -> bool;
    fn pressed_button(&mut self, timeout: Duration) -> Result<Button>;
    fn set_time(&mut self, adjustment_secs: i64, timeout: Duration) -> Result<chrono::DateTime<Local>>;
    fn disconnect(&mut self) -> Result<()>;
}

pub trait BluetoothBackend {
    fn scan_and_connect(&mut self, timeout: Duration, accept: &dyn Fn(&str) -> bool, stop: &dyn Fn() -> bool) -> Result<Box<dyn ConnectedWatch>>;
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
        let (tmp, mut file) = create_temp_file(&self.path)?;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()).and_then(|_| fs::rename(&tmp, &self.path)) {
            let _ = fs::remove_file(&tmp);
            return Err(error.into());
        }
        self.data = data;
        Ok(())
    }
}

fn create_temp_file(path: &Path) -> Result<(PathBuf, fs::File), std::io::Error> {
    static NEXT_TEMP: OnceLock<AtomicU64> = OnceLock::new();
    let sequence = NEXT_TEMP.get_or_init(|| AtomicU64::new(0));
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("state");
    let directory = path.parent().unwrap_or_else(|| Path::new("."));

    for _ in 0..100 {
        let id = sequence.fetch_add(1, Ordering::Relaxed);
        let candidate = directory.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), id));
        match fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "could not create unique state temp file"))
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
            if stop() {
                break;
            }
            info!("Waiting for connection...");
            let accept = |name: &str| name != "CASIO OCW-T200" && self.limiter.allow(name);
            let mut watch = match self.backend.scan_and_connect(self.config.scan_timeout, &accept, &stop) {
                Ok(w) => w,
                Err(_) if stop() => break,
                Err(e) => {
                    if is_no_matching_watch(&e) {
                        debug!("no matching watch found");
                    } else {
                        error!("connection failed: {e:#}");
                    }
                    continue;
                }
            };
            if stop() {
                if !watch.always_connected() {
                    watch.disconnect().unwrap_or_else(|e| warn!("disconnect failed: {e:#}"));
                }
                break;
            }
            info!("Connected watch={} address={}", watch.name(), watch.address());
            let state = State { last_connected: Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()), watch_name: Some(watch.name().to_string()) };
            if let Err(e) = store.update(state) {
                warn!("state file could not be saved: {e}");
            }
            match watch.pressed_button(self.config.request_timeout) {
                Ok(button) if should_set_time(button) => {
                    info!("Set time start watch={} adjust={}", watch.name(), self.config.fine_adjustment_secs);
                    match watch.set_time(self.config.fine_adjustment_secs, self.config.request_timeout) {
                        Ok(time) => {
                            info!("Set time done watch={} time={} adjust={}", watch.name(), time.to_rfc3339(), self.config.fine_adjustment_secs)
                        }
                        Err(e) => error!("Set time failed watch={}: {e:#}", watch.name()),
                    }
                }
                Ok(_) => info!("connection ignored: unsupported button"),
                Err(e) => error!("button query failed: {e:#}"),
            }
            if !watch.always_connected() {
                watch.disconnect().unwrap_or_else(|e| warn!("disconnect failed: {e:#}"));
            }
        }
        info!("Server stopped");
        Ok(())
    }
}

fn is_no_matching_watch(error: &AnyhowError) -> bool {
    error.chain().any(|cause| cause.to_string() == "no matching watch found")
}

fn should_set_time(button: Button) -> bool {
    matches!(button, Button::LowerLeft | Button::LowerRight | Button::NoButton)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn store_round_trip() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.json");
        let mut store = Store::open(&path).unwrap();
        let state = State { last_connected: Some("08/18 12:34".into()), watch_name: Some("CASIO GW-B5600".into()) };
        store.update(state.clone()).unwrap();
        assert_eq!(Store::open(path).unwrap().data, state);
    }

    #[test]
    fn limiter() {
        let limiter = ConnectionLimiter::new();
        assert!(limiter.allow("CASIO ECB-30"));
        assert!(!limiter.allow("CASIO ECB-30"));
        assert!(limiter.allow("CASIO GW-B5600"));
        assert!(limiter.allow("CASIO GW-B5600"));
    }

    #[test]
    fn automatic_connection_sets_time_without_a_button() {
        assert!(should_set_time(Button::NoButton));
        assert!(should_set_time(Button::LowerLeft));
        assert!(should_set_time(Button::LowerRight));
        assert!(!should_set_time(Button::Invalid));
    }
}
