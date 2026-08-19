use chrono::{DateTime, Datelike, Offset, Timelike};

pub const FEATURE_BLE: u8 = 0x10;
pub const FEATURE_DST_STATE: u8 = 0x1d;
pub const FEATURE_DST_CITY: u8 = 0x1e;
pub const FEATURE_WORLD: u8 = 0x1f;
pub const FEATURE_HOME_TIME: u8 = 0x24;
pub const FEATURE_TIME: u8 = 0x09;

pub fn encode_time<Tz: chrono::TimeZone>(time: &DateTime<Tz>) -> Vec<u8> {
    let mut packet = vec![0; 11];
    packet[0] = FEATURE_TIME;
    let year = time.year() as u16;
    packet[1..3].copy_from_slice(&year.to_le_bytes());
    packet[3..8].copy_from_slice(&[
        time.month() as u8,
        time.day() as u8,
        time.hour() as u8,
        time.minute() as u8,
        time.second() as u8,
    ]);
    packet[8] = time.weekday().num_days_from_monday() as u8;
    packet[9] = ((time.nanosecond() as u64 * 256) / 1_000_000_000) as u8;
    packet[10] = 1;
    packet
}

pub fn encode_mip_time<Tz: chrono::TimeZone>(time: &DateTime<Tz>) -> Vec<u8> {
    let mut packet = encode_time(time);
    packet[8] = time.weekday().number_from_monday() as u8;
    packet
}

pub fn response_key(data: &[u8], analogue: bool) -> Option<u8> {
    let first = *data.first()?;
    if !analogue || first != 0x28 {
        return Some(first);
    }
    match data.get(1) {
        Some(1) => data.get(4).copied(),
        Some(0) => data.get(3).copied(),
        _ => Some(first),
    }
}

pub fn unwrap_response(data: &[u8], key: u8, analogue: bool) -> &[u8] {
    if !analogue || data.len() < 2 || data[0] != 0x28 || key == 0x28 {
        return data;
    }
    if data[1] == 1 && data.len() >= 4 {
        return &data[4..];
    }
    if data.len() >= 3 {
        return &data[3..];
    }
    data
}

pub fn world_city_records<Tz: chrono::TimeZone>(now: DateTime<Tz>) -> Vec<u8> {
    let offset = now.offset().fix().local_minus_utc() as f64;
    let longitude = (offset / 3600.0 * 15.0).clamp(-180.0, 180.0);
    let mut result = Vec::with_capacity(66);
    result.extend(city_record(0, 0.0, longitude, 0));
    result.extend(city_record(1, 0.0, 0.0, 0));
    result.extend(city_record(2, 0.0, 0.0, 0));
    result
}

fn city_record(slot: u8, latitude: f64, longitude: f64, trailing: u8) -> Vec<u8> {
    let mut record = vec![0x14, 0, 0x24, slot, 1];
    record.extend(latitude.to_be_bytes());
    record.extend(longitude.to_be_bytes());
    record.push(trailing);
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn time_packet() {
        let time = Utc.with_ymd_and_hms(2023, 1, 2, 3, 4, 5).unwrap();
        assert_eq!(encode_time(&time), vec![9, 0xe7, 7, 1, 2, 3, 4, 5, 0, 0, 1]);
    }

    #[test]
    fn mip_sunday_is_seven() {
        let time = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(encode_mip_time(&time)[8], 7);
    }

    #[test]
    fn envelope() {
        let data = [0x28, 1, 0xaa, 0xbb, 0x1d, 2];
        assert_eq!(response_key(&data, true), Some(0x1d));
        assert_eq!(unwrap_response(&data, 0x1d, true), &data[4..]);
    }

    #[test]
    fn records_are_three_22_byte_records() {
        assert_eq!(world_city_records(Utc::now()).len(), 66);
    }
}
