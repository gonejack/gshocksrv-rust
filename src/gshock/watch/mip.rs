use super::{Watch, WatchError, WatchIo};
use crate::gshock::protocol::*;
use chrono::{DateTime, Local};

impl<I: WatchIo> Watch<I> {
    pub(super) fn set_time_mip(&mut self, adjustment: i64) -> Result<DateTime<Local>, WatchError> {
        let mut step1 = self
            .io
            .request_sp(&[5, 0x1d, 0, 0x1d, 0, 0x24, 0, 0x24, 1, 0x24, 2], 101, self.request_timeout)
            .map_err(|e| e.context("MIP step 1"))?;
        step1[0] = 2;
        self.io.write_sp(&step1).map_err(|e| e.context("MIP step 1 write"))?;

        let mut step2_request = vec![3];
        for _ in 0..self.profile.world_cities.div_ceil(2) {
            step2_request.extend([FEATURE_DST_CITY, 0]);
        }
        let mut step2 = self.io.request_sp(&step2_request, 28, self.request_timeout).map_err(|e| e.context("MIP step 2"))?;
        step2[0] = 6;
        step2.extend(world_city_records(Local::now()));
        self.io.write_sp(&step2).map_err(|e| e.context("MIP step 2 write"))?;

        let mut step3_request = vec![6];
        for i in 0..self.profile.world_cities {
            let mut index = (i / 2) as u8;
            if i % 2 == 1 {
                index += 6;
            }
            step3_request.extend([FEATURE_WORLD, index]);
        }
        let step3 = self
            .io
            .request_sp(&step3_request, 1 + self.profile.world_cities * 22, self.request_timeout)
            .map_err(|e| e.context("MIP step 3"))?;
        self.io.write_sp(&step3).map_err(|e| e.context("MIP step 3 write"))?;

        let when = Local::now() + chrono::Duration::seconds(adjustment);
        self.write_time(&encode_mip_time(&when)).map_err(|e| e.context("MIP time write"))?;
        Ok(when)
    }
}
