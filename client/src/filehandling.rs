use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Debug)]
pub struct StaticKeys {
    pub ed_static_material: [u8; 32],
    pub as_keys: Vec<AsKeys>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AsKeys {
    pub kid: Vec<u8>,
    pub as_static_material: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Config {
    pub deveui: [u8; 8],
    pub appeui: [u8; 8],
    pub dhr_const: u16,
    pub rx1_delay: u64,
    pub rx1_duration: i32,
    pub rx2_delay: u64,
    pub rx2_duration: i32,
}

fn load_file(path: String) -> String {
    fs::read_to_string(path).expect("Unable to read file")
}

pub fn load_static_keys(path: String) -> StaticKeys {
    let static_data = load_file(path);
    let static_keys: StaticKeys = serde_json::from_str(&static_data).unwrap();
    static_keys
}

pub fn load_config(path: String) -> Config {
    let config_data = load_file(path);
    let config: Config = serde_json::from_str(&config_data).unwrap();
    config
}