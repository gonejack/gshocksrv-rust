use super::protocol::*;
use super::{Button, Profile, Protocol};
use chrono::{DateTime, Local};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("request timed out")]
    Timeout,
    #[error("watch lacks MIP protocol characteristics")]
    MissingMip,
}

pub trait WatchIo {
    fn write(&mut self, data: &[u8], without_response: bool) -> Result<(), WatchError>;
    fn read_response(&mut self, expected: u8, timeout: Duration, analogue: bool) -> Result<Vec<u8>, WatchError>;
    fn request_sp(&mut self, request: &[u8], expected_len: usize, timeout: Duration) -> Result<Vec<u8>, WatchError>;
    fn write_sp(&mut self, data: &[u8]) -> Result<(), WatchError>;
}

pub struct Watch<I> {
    pub name: String,
    pub address: String,
    pub profile: Profile,
    pub io: I,
    pub request_timeout: Duration,
}
impl<I: WatchIo> Watch<I> {
    pub fn pressed_button(&mut self) -> Result<Button, WatchError> {
        self.io.write(&[FEATURE_BLE], true)?;
        Ok(super::decode_button(&self.io.read_response(FEATURE_BLE, self.request_timeout, self.profile.protocol == Protocol::Analogue)?))
    }
    fn request(&mut self, request: &[u8], key: u8) -> Result<Vec<u8>, WatchError> {
        self.io.write(request, true)?;
        self.io.read_response(key, self.request_timeout, self.profile.protocol == Protocol::Analogue)
    }
    fn round_trip(&mut self, request: &[u8], key: u8) -> Result<(), WatchError> {
        let r = self.request(request, key)?;
        self.io.write(&r, false)
    }
    pub fn set_time(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
        if self.profile.protocol == Protocol::Mip {
            return self.set_time_mip(adjustment);
        }
        for state in [0u8, 2, 4].into_iter().take(self.profile.dst_states) {
            self.round_trip(&[FEATURE_DST_STATE, state], FEATURE_DST_STATE)?;
        }
        for city in 0..self.profile.world_cities as u8 {
            self.round_trip(&[FEATURE_DST_CITY, city], FEATURE_DST_CITY)?;
        }
        let feature = if self.profile.has_world_cities {
            FEATURE_WORLD
        } else if self.profile.has_home_time {
            FEATURE_HOME_TIME
        } else {
            0
        };
        if feature != 0 {
            for city in 0..self.profile.world_cities as u8 {
                self.round_trip(&[feature, city], feature)?;
            }
        }
        let when = Local::now() + chrono::Duration::seconds(adjustment);
        self.io.write(&encode_time(when), false)?;
        Ok(when)
    }
    fn set_time_mip(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
        let mut r = self.io.request_sp(&[5, 0x1d, 0, 0x1d, 0, 0x24, 0, 0x24, 1, 0x24, 2], 101, self.request_timeout)?;
        r[0] = 2;
        self.io.write_sp(&r)?;
        let mut req = vec![3];
        for _ in 0..((self.profile.world_cities + 1) / 2) {
            req.extend([FEATURE_DST_CITY, 0]);
        }
        let mut r = self.io.request_sp(&req, 28, self.request_timeout)?;
        r[0] = 6;
        r.extend(world_city_records(Local::now()));
        self.io.write_sp(&r)?;
        let mut req = vec![6];
        for i in 0..self.profile.world_cities {
            let mut index = (i / 2) as u8;
            if i % 2 == 1 {
                index += 6;
            }
            req.extend([FEATURE_WORLD, index]);
        }
        let r = self.io.request_sp(&req, 1 + self.profile.world_cities * 22, self.request_timeout)?;
        self.io.write_sp(&r)?;
        let when = Local::now() + chrono::Duration::seconds(adjustment);
        self.io.write(&encode_mip_time(when), false)?;
        Ok(when)
    }
}
