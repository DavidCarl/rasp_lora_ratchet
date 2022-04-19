use serde::{Deserialize, Serialize};

use std::fs;


#[derive(Serialize, Deserialize, Debug)]
pub struct StaticKeys {
    pub as_static_material: [u8; 32],
    pub ed_keys: Vec<EdKeys>, //ed_static_pk_material: [u8; 32]
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EdKeys {
    pub kid: Vec<u8>,
    pub ed_static_material: [u8; 32],
}

/// Get the content from a text file
///     
/// # Arguments
///
/// * `path` - A string for where the file are located
fn load_file(path: String) -> String {
    fs::read_to_string(path).expect("Unable to read file")
}

/// Convert a files content to a StaticKeys struct
///
/// # Arguments
///
/// *  `path` - A string for where the file are located
pub fn load_static_keys(path: String) -> StaticKeys {
    let static_data = load_file(path);
    let static_keys: StaticKeys = serde_json::from_str(&static_data).unwrap();
    static_keys
}
