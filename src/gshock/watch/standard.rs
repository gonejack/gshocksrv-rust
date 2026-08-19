use super::{Watch, WatchError, WatchIo};
use crate::gshock::protocol::*;
use chrono::{DateTime, Local};

impl<I: WatchIo> Watch<I> {
    pub(super) fn set_time_std(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
        for state in [0u8, 2, 4].into_iter().take(self.profile.dst_states) {
            self.round_trip(&[FEATURE_DST_STATE, state], FEATURE_DST_STATE)?;
        }
        for city in 0..self.profile.world_cities as u8 {
            self.round_trip(&[FEATURE_DST_CITY, city], FEATURE_DST_CITY)?;
        }

        let feature = if self.profile.has_world_cities {
            Some(FEATURE_WORLD)
        } else if self.profile.has_home_time {
            Some(FEATURE_HOME_TIME)
        } else {
            None
        };
        if let Some(feature) = feature {
            for city in 0..self.profile.world_cities as u8 {
                self.round_trip(&[feature, city], feature)?;
            }
        }

        let when = Local::now() + chrono::Duration::seconds(adjustment);
        self.write_time(&encode_time(&when))?;
        if self.profile.second_dial {
            self.set_second_dial()?;
        }
        Ok(when)
    }

    fn set_second_dial(&mut self) -> Result<(), WatchError> {
        self.io.write(&[0x21, 0x00, 0x01], false)?;
        self.round_trip(&[FEATURE_DST_STATE, 0], FEATURE_DST_STATE)?;
        for feature in [FEATURE_DST_CITY, FEATURE_WORLD] {
            if feature == FEATURE_WORLD && !self.profile.has_world_cities {
                continue;
            }
            for city in 0..2u8 {
                self.round_trip(&[feature, city], feature)?;
            }
        }
        self.io.write(&[0x21, 0x01, 0x01], false)
    }
}
