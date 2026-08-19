use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Standard,
    Analogue,
    Mip,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    pub protocol: Protocol,
    pub world_cities: usize,
    pub dst_states: usize,
    pub has_world_cities: bool,
    pub has_home_time: bool,
    pub second_dial: bool,
    pub always_connected: bool,
}

pub fn profile_for(name: &str) -> Profile {
    let model = name.trim().strip_prefix("CASIO ").unwrap_or(name.trim());
    let mut p = Profile {
        protocol: Protocol::Standard,
        world_cities: 2,
        dst_states: 1,
        has_world_cities: true,
        has_home_time: false,
        second_dial: false,
        always_connected: false,
    };
    match model {
        "GW-BX5600" => {
            p.protocol = Protocol::Mip;
            p.world_cities = 6;
            p.dst_states = 3;
        }
        "MTG-B1000" => {
            p.protocol = Protocol::Analogue;
            p.world_cities = 6;
            p.dst_states = 3;
            p.second_dial = true;
        }
        "MTG-B3000" | "MTG-B3100" => {
            p.protocol = Protocol::Analogue;
            p.has_world_cities = false;
            p.has_home_time = true;
            p.second_dial = true;
        }
        _ => {}
    }
    if six_city(model) {
        p.world_cities = 6;
        p.dst_states = 3;
    }
    if no_city(model) {
        p.has_world_cities = false;
    }
    p.always_connected = is_always_connected(model);
    p
}
fn six_city(model: &str) -> bool {
    matches!(
        model,
        "GW-B5000"
            | "GW-B5600"
            | "GW-B5600#"
            | "GW-BX5600"
            | "GMW-B5000"
            | "GMW-B5000#"
            | "GMW-BZ5000"
            | "MRG-B5000"
            | "MRG-B5000#"
            | "GCW-B5000"
            | "TRN-50"
            | "PRJ-BW002"
            | "DW-B5600"
            | "GWR-B1000"
            | "GWR-B3000"
            | "GWG-B1000"
    )
}
fn no_city(model: &str) -> bool {
    ["ABL-", "GBD-", "GBX-", "GMD-B800", "EQB-"].iter().any(|p| model.starts_with(p))
}
pub fn is_always_connected(name: &str) -> bool {
    let m = name.trim().strip_prefix("CASIO ").unwrap_or(name.trim());
    matches!(m, "DW-H5600" | "GBD-H2000" | "DW-GH5600" | "GM-H5600") || m.starts_with("ECB-")
}
impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Standard => "standard",
                Self::Analogue => "analogue",
                Self::Mip => "mip",
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_profiles() {
        let p = profile_for("CASIO GW-BX5600");
        assert_eq!((p.protocol, p.world_cities, p.dst_states), (Protocol::Mip, 6, 3));
        assert!(profile_for("CASIO ECB-30").always_connected);
    }
}
