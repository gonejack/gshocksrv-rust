mod profile;
pub mod protocol;
pub mod watch;
pub use profile::{Profile, Protocol, is_always_connected, profile_for};

pub const CASIO_SERVICE_UUID: &str = "00001804-0000-1000-8000-00805f9b34fb";
pub const READ_REQUEST_UUID: &str = "26eb002c-b012-49a8-b1f8-394fb2032b0f";
pub const ALL_FEATURES_UUID: &str = "26eb002d-b012-49a8-b1f8-394fb2032b0f";
pub const SP_REQUEST_UUID: &str = "26eb002e-b012-49a8-b1f8-394fb2032b0f";
pub const SP_DATA_UUID: &str = "26eb002f-b012-49a8-b1f8-394fb2032b0f";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Invalid,
    LowerLeft,
    LowerRight,
    NoButton,
}

pub fn decode_button(data: &[u8]) -> Button {
    if data.len() < 19 || data[0] != 0x10 {
        return Button::Invalid;
    }
    match data[8] {
        0 | 1 => Button::LowerLeft,
        3 => Button::NoButton,
        4 => Button::LowerRight,
        _ => Button::Invalid,
    }
}
