// The Halloween maps from TF2's casual menu. Same file the UI and the Node server read.

use std::sync::LazyLock;

use serde::Deserialize;

// Only the fields Rust reads. serde skips the rest (names, tabs, images, wiki links), which the UI reads from the same file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HalloweenMap {
    /// BSP name (e.g. koth_harvest_event). Stable key for progress.
    pub code: String,
    /// master_maps_list index; the map's bit in casual_criteria.vdf's selected_maps_bits.
    pub map_index: u32,
    /// ConTracker node defindex for this map's Halloween contract.
    pub contract_node: u32,
}

pub static MAPS: LazyLock<Vec<HalloweenMap>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/halloween-maps.json"))
        .expect("halloween-maps.json is embedded at build time and must match HalloweenMap")
});

pub fn by_code(code: &str) -> Option<&'static HalloweenMap> {
    MAPS.iter().find(|map| map.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_map_by_code() {
        let map = by_code("cp_holyhell").expect("cp_holyhell is in the list");
        assert_eq!(map.map_index, 231);
        assert!(by_code("not_a_map").is_none());
    }
}
