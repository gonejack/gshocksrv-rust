mod mip;
mod standard;

use super::protocol::*;
use super::{Button, Profile, Protocol};
use chrono::{DateTime, Local};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("watch disconnected")]
    Disconnected,
    #[error("request timed out")]
    Timeout,
    #[error("operation interrupted")]
    Interrupted,
    #[error("watch lacks MIP protocol characteristics")]
    MissingMip,
    #[error("{operation}: {source}")]
    Context {
        operation: String,
        #[source]
        source: Box<Self>,
    },
}

impl WatchError {
    pub(super) fn context(self, operation: impl Into<String>) -> Self {
        Self::Context { operation: operation.into(), source: Box::new(self) }
    }
}

pub trait WatchIo {
    fn write(&mut self, data: &[u8], without_response: bool) -> Result<(), WatchError>;
    fn read_response(&mut self, expected: u8, timeout: Duration, analogue: bool) -> Result<Vec<u8>, WatchError>;
    fn request_sp(&mut self, request: &[u8], expected_len: usize, timeout: Duration) -> Result<Vec<u8>, WatchError>;
    fn write_sp(&mut self, data: &[u8]) -> Result<(), WatchError>;

    /// A time packet can make some watches disconnect immediately after it is received.
    fn is_connected(&self) -> Result<bool, WatchError> {
        Ok(true)
    }
}

pub struct Watch<I> {
    pub name: String,
    pub identifier: String,
    pub profile: Profile,
    pub io: I,
    pub request_timeout: Duration,
}

impl<I: WatchIo> Watch<I> {
    pub fn pressed_button(&mut self) -> Result<Button, WatchError> {
        self.io.write(&[FEATURE_BLE], true)?;
        let response = self.io.read_response(FEATURE_BLE, self.request_timeout, self.profile.protocol == Protocol::Analogue)?;
        Ok(super::decode_button(&response))
    }

    pub(super) fn request(&mut self, request: &[u8], key: u8) -> Result<Vec<u8>, WatchError> {
        self.io.write(request, true)?;
        self.io.read_response(key, self.request_timeout, self.profile.protocol == Protocol::Analogue)
    }

    pub(super) fn round_trip(&mut self, request: &[u8], key: u8) -> Result<(), WatchError> {
        self.request(request, key)
            .and_then(|response| self.io.write(&response, false))
            .map_err(|error| error.context(format!("feature 0x{key:02x} item {}", request.get(1).copied().unwrap_or_default())))
    }

    pub fn set_time(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
        match self.profile.protocol {
            Protocol::Mip => self.set_time_mip(adjustment),
            Protocol::Standard | Protocol::Analogue => self.set_time_std(adjustment),
        }
    }

    pub(super) fn write_time(&mut self, data: &[u8]) -> Result<(), WatchError> {
        let result = self.io.write(data, false);
        if result.is_ok() {
            return Ok(());
        }
        if matches!(result, Err(WatchError::Disconnected)) {
            return Ok(());
        }

        // Several Casio models disconnect as soon as the time packet is accepted.
        if self.io.is_connected().ok() == Some(false) {
            return Ok(());
        }
        result.map_err(|error| error.context("write time packet"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Default)]
    struct RecordingIo {
        writes: Vec<Vec<u8>>,
        disconnect_on_time: bool,
        disconnected_error_on_time: bool,
        disconnect_before_time: bool,
        connection_checks: Cell<usize>,
    }

    impl WatchIo for RecordingIo {
        fn write(&mut self, data: &[u8], without_response: bool) -> Result<(), WatchError> {
            if !without_response && self.disconnected_error_on_time && data.first() == Some(&FEATURE_TIME) {
                return Err(WatchError::Disconnected);
            }
            if !without_response && self.disconnect_on_time && data.first() == Some(&FEATURE_TIME) {
                return Err(WatchError::Transport("watch disconnected after time packet".into()));
            }
            self.writes.push(data.to_vec());
            Ok(())
        }

        fn read_response(&mut self, expected: u8, _timeout: Duration, _analogue: bool) -> Result<Vec<u8>, WatchError> {
            if self.disconnect_before_time && expected == FEATURE_DST_STATE {
                return Err(WatchError::Disconnected);
            }
            Ok(vec![expected])
        }

        fn request_sp(&mut self, _request: &[u8], _expected_len: usize, _timeout: Duration) -> Result<Vec<u8>, WatchError> {
            Err(WatchError::MissingMip)
        }

        fn write_sp(&mut self, _data: &[u8]) -> Result<(), WatchError> {
            Err(WatchError::MissingMip)
        }

        fn is_connected(&self) -> Result<bool, WatchError> {
            self.connection_checks.set(self.connection_checks.get() + 1);
            Ok(!self.disconnect_on_time)
        }
    }

    #[test]
    fn analogue_second_dial_is_configured() {
        let mut watch = Watch {
            name: "CASIO MTG-B3000".into(),
            identifier: "test".into(),
            profile: super::super::profile_for("CASIO MTG-B3000"),
            io: RecordingIo { disconnect_on_time: true, ..Default::default() },
            request_timeout: Duration::from_secs(1),
        };

        watch.set_time(0).unwrap();
        let writes = &watch.io.writes;
        assert!(writes.iter().any(|data| data == &[0x21, 0x00, 0x01]));
        assert!(writes.iter().any(|data| data == &[0x21, 0x01, 0x01]));
    }

    #[test]
    fn known_disconnect_during_time_write_is_accepted() {
        let mut watch = Watch {
            name: "CASIO GW-B5600".into(),
            identifier: "test".into(),
            profile: super::super::profile_for("CASIO GW-B5600"),
            io: RecordingIo { disconnected_error_on_time: true, ..Default::default() },
            request_timeout: Duration::from_secs(1),
        };

        watch.set_time(0).unwrap();
        assert_eq!(watch.io.connection_checks.get(), 0);
    }

    #[test]
    fn disconnect_before_time_packet_is_an_error() {
        let mut watch = Watch {
            name: "CASIO GW-B5600".into(),
            identifier: "test".into(),
            profile: super::super::profile_for("CASIO GW-B5600"),
            io: RecordingIo { disconnect_before_time: true, ..Default::default() },
            request_timeout: Duration::from_secs(1),
        };

        assert!(watch.set_time(0).is_err());
        assert!(!watch.io.writes.iter().any(|data| data.first() == Some(&FEATURE_TIME)));
    }
}
