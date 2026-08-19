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
        let response = self.io.read_response(FEATURE_BLE, self.request_timeout, self.profile.protocol == Protocol::Analogue)?;
        Ok(super::decode_button(&response))
    }

    fn request(&mut self, request: &[u8], key: u8) -> Result<Vec<u8>, WatchError> {
        self.io.write(request, true)?;
        self.io.read_response(key, self.request_timeout, self.profile.protocol == Protocol::Analogue)
    }

    fn round_trip(&mut self, request: &[u8], key: u8) -> Result<(), WatchError> {
        let response = self.request(request, key)?;
        self.io.write(&response, false)
    }

    pub fn set_time(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
        match self.profile.protocol {
            Protocol::Mip => self.set_time_mip(adjustment),
            Protocol::Standard | Protocol::Analogue => self.set_time_std(adjustment),
        }
    }

    fn set_time_std(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
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
        self.io.write(&encode_time(&when), false)?;
        Ok(when)
    }

    fn set_time_mip(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
        let mut step1 = self.io.request_sp(&[5, 0x1d, 0, 0x1d, 0, 0x24, 0, 0x24, 1, 0x24, 2], 101, self.request_timeout)?;
        step1[0] = 2;
        self.io.write_sp(&step1)?;

        let mut step2_request = vec![3];
        for _ in 0..self.profile.world_cities.div_ceil(2) {
            step2_request.extend([FEATURE_DST_CITY, 0]);
        }
        let mut step2 = self.io.request_sp(&step2_request, 28, self.request_timeout)?;
        step2[0] = 6;
        step2.extend(world_city_records(Local::now()));
        self.io.write_sp(&step2)?;

        let mut step3_request = vec![6];
        for i in 0..self.profile.world_cities {
            let mut index = (i / 2) as u8;
            if i % 2 == 1 {
                index += 6;
            }
            step3_request.extend([FEATURE_WORLD, index]);
        }
        let step3 = self.io.request_sp(&step3_request, 1 + self.profile.world_cities * 22, self.request_timeout)?;
        self.io.write_sp(&step3)?;

        let when = Local::now() + chrono::Duration::seconds(adjustment);
        self.io.write(&encode_mip_time(&when), false)?;
        Ok(when)
    }
}
