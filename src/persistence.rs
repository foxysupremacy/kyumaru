use crate::config::save_root;
use crate::log;
use serde_json::Value;
use std::fs::{create_dir_all, File};
use std::io::Write;

pub fn save_inventory_data(data: &Value) {
    let dir = save_root();
    if let Err(e) = create_dir_all(dir) {
        log!("[Persistence] Failed to create dir {:?}: {}", dir, e);
        return;
    }

    let path = dir.join("inventory.json");
    log!("[Persistence] Saving inventory to: {}", path.display());

    match File::create(&path) {
        Ok(mut f) => match serde_json::to_string_pretty(data) {
            Ok(json_str) => {
                if let Err(e) = write!(f, "{}", json_str) {
                    log!("[Persistence] Failed to write inventory JSON: {}", e);
                } else {
                    log!("[Persistence] Inventory saved successfully to: {}", path.display());
                }
            }
            Err(e) => {
                log!("[Persistence] Failed to serialize inventory JSON: {}", e);
            }
        },
        Err(e) => {
            log!("[Persistence] Failed to create inventory file: {}", e);
        }
    }
}

pub fn save_veteran_data(list_data: &Value) {
    if !list_data.is_array() {
        log!("[Persistence] Warning: Veteran data is not an array");
        return;
    }

    if let Value::Array(ref arr) = list_data {
        if arr.is_empty() {
            log!("[Persistence] No veteran characters to save (empty array)");
            return;
        }
        log!("[Persistence] Saving {} veteran character(s)", arr.len());
    }

    let dir = save_root();
    if let Err(e) = create_dir_all(dir) {
        log!("[Persistence] Failed to create dir {:?}: {}", dir, e);
        return;
    }

    let path = dir.join("veterans.json");

    match File::create(&path) {
        Ok(mut f) => match serde_json::to_string_pretty(list_data) {
            Ok(json_str) => {
                if let Err(e) = write!(f, "{}", json_str) {
                    log!("[Persistence] Failed to write veterans JSON: {}", e);
                } else {
                    log!("[Persistence] Veterans saved successfully to: {}", path.display());
                }
            }
            Err(e) => {
                log!("[Persistence] Failed to serialize veterans JSON: {}", e);
            }
        },
        Err(e) => {
            log!("[Persistence] Failed to create veterans file: {}", e);
        }
    }
}
