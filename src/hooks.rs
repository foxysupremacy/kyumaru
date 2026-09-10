use crate::il2cpp::*;
use crate::log;
use crate::persistence::{save_inventory_data, save_veteran_data};
use crate::plugin_api::*;
use crate::reflection::convert_object_to_value;
use crate::sync::{compress_and_encode, open_browser_with_payload};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::transmute;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub static mut ORIG_LZ4_DECOMPRESS: usize = 0;
pub static mut ORIG_LOAD_INDEX: usize = 0;
pub static mut ORIG_VETERAN_APPLY: usize = 0;

pub static IS_CAPTURED: AtomicBool = AtomicBool::new(false);
pub static CARD_COUNT: Mutex<usize> = Mutex::new(0);
pub static UMA_COUNT: Mutex<usize> = Mutex::new(0);
pub static COMPRESSED_PAYLOAD: Mutex<Option<String>> = Mutex::new(None);

pub type FnLz4DecompressSafeExt =
    unsafe extern "C" fn(src: *const u8, dst: *mut u8, compressed_size: i32, dst_capacity: i32) -> i32;

/// Native hook for LZ4_decompress_safe_ext in libnative.dll
pub unsafe extern "C" fn lz4_decompress_safe_ext_hook(
    src: *const u8,
    dst: *mut u8,
    compressed_size: i32,
    dst_capacity: i32,
) -> i32 {
    if ORIG_LZ4_DECOMPRESS == 0 {
        return 0;
    }
    let orig: FnLz4DecompressSafeExt = transmute(ORIG_LZ4_DECOMPRESS);
    let ret = orig(src, dst, compressed_size, dst_capacity);

    if ret <= 0 || dst.is_null() {
        return ret;
    }

    let slice = std::slice::from_raw_parts(dst, ret as usize);

    // Fast check (<5µs): check if the decompressed packet contains b"support_card_list"
    if find_subsequence(slice, b"support_card_list") {
        let buffer = slice.to_vec();
        std::thread::spawn(move || {
            process_load_index_payload(&buffer);
        });
    }

    ret
}

/// Fast subsequence check for byte slices
pub fn find_subsequence(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    let first = needle[0];
    let needle_len = needle.len();
    let mut offset = 0;
    while offset + needle_len <= haystack.len() {
        if let Some(pos) = haystack[offset..].iter().position(|&b| b == first) {
            let start = offset + pos;
            if start + needle_len > haystack.len() {
                return false;
            }
            if &haystack[start..start + needle_len] == needle {
                return true;
            }
            offset = start + 1;
        } else {
            return false;
        }
    }
    false
}

/// Converts rmpv::Value recursively to serde_json::Value
pub fn rmpv_to_json(val: &rmpv::Value) -> Value {
    match val {
        rmpv::Value::Nil => Value::Null,
        rmpv::Value::Boolean(b) => Value::Bool(*b),
        rmpv::Value::Integer(i) => {
            if let Some(v) = i.as_i64() {
                json!(v)
            } else if let Some(v) = i.as_u64() {
                json!(v)
            } else {
                Value::Null
            }
        }
        rmpv::Value::F32(f) => json!(*f),
        rmpv::Value::F64(f) => json!(*f),
        rmpv::Value::String(s) => {
            if let Some(st) = s.as_str() {
                Value::String(st.to_string())
            } else {
                Value::Null
            }
        }
        rmpv::Value::Binary(b) => Value::Array(b.iter().map(|&byte| json!(byte)).collect()),
        rmpv::Value::Array(arr) => Value::Array(arr.iter().map(rmpv_to_json).collect()),
        rmpv::Value::Map(entries) => {
            let mut map = Map::new();
            for (k, v) in entries {
                if let Some(key_str) = k.as_str() {
                    map.insert(key_str.to_string(), rmpv_to_json(v));
                }
            }
            Value::Object(map)
        }
        rmpv::Value::Ext(_, _) => Value::Null,
    }
}

/// Parses the decompressed load/index MessagePack payload, dumps inventory.json, and encodes sync URL
pub fn process_load_index_payload(data: &[u8]) {
    let _ = std::panic::catch_unwind(|| {
        log!("[Kyumaru] Detected load/index packet! Unpacking MessagePack...");
        let mut cursor = std::io::Cursor::new(data);
        let val = match rmpv::decode::read_value(&mut cursor) {
            Ok(v) => v,
            Err(e) => {
                log!("[Kyumaru] Error decoding MessagePack: {}", e);
                return;
            }
        };

        let json_val = rmpv_to_json(&val);
        let data_obj = match json_val.get("data") {
            Some(d) if d.is_object() => d,
            _ => {
                log!("[Kyumaru] No 'data' object found in packet.");
                return;
            }
        };

        let mut card_map = Map::new();
        if let Some(cards) = data_obj.get("support_card_list").and_then(Value::as_array) {
            for card in cards {
                if let (Some(id), Some(lb)) = (
                    card.get("support_card_id").and_then(Value::as_i64),
                    card.get("limit_break_count").and_then(Value::as_i64),
                ) {
                    card_map.insert(id.to_string(), json!(lb));
                }
            }
        }

        let mut uma_map = Map::new();
        if let Some(umas) = data_obj.get("card_list").and_then(Value::as_array) {
            for uma in umas {
                if let (Some(id), Some(rarity), Some(talent)) = (
                    uma.get("card_id").and_then(Value::as_i64),
                    uma.get("rarity").and_then(Value::as_i64),
                    uma.get("talent_level").and_then(Value::as_i64),
                ) {
                    uma_map.insert(id.to_string(), json!([rarity, talent]));
                }
            }
        }

        let card_len = card_map.len();
        let uma_len = uma_map.len();

        if card_len > 0 {
            let full_inventory = json!({
                "support_card_list": data_obj.get("support_card_list").cloned().unwrap_or(Value::Null),
                "card_list": data_obj.get("card_list").cloned().unwrap_or(Value::Null),
                "user_info": data_obj.get("user_info").cloned().unwrap_or(Value::Null),
            });

            // 1. Save inventory.json to Documents/Kyumaru
            save_inventory_data(&full_inventory);

            // 2. If trained_chara exists in this load/index packet, save veterans.json
            if let Some(veterans) = data_obj.get("trained_chara") {
                if veterans.is_array() {
                    save_veteran_data(veterans);
                }
            }

            // 3. Compress compact sync payload for URL
            let sync_payload = json!({
                "cards": card_map,
                "umas": uma_map,
            });

            if let Ok(b64) = compress_and_encode(&sync_payload) {
                *COMPRESSED_PAYLOAD.lock().unwrap() = Some(b64);
                *CARD_COUNT.lock().unwrap() = card_len;
                *UMA_COUNT.lock().unwrap() = uma_len;
                IS_CAPTURED.store(true, Ordering::SeqCst);
                log!(
                    "[Kyumaru] Inventory captured successfully: {} support cards, {} characters.",
                    card_len,
                    uma_len
                );
                unsafe {
                    show_toast(&format!(
                        "Kyumaru: Captured {} support cards, {} characters!",
                        card_len, uma_len
                    ));
                }
            }
        }
    });
}

pub unsafe extern "C" fn veteran_hook(
    this: *mut RawIl2CppObject,
    trained_chara_array: *mut RawIl2CppObject,
) {
    if ORIG_VETERAN_APPLY != 0 {
        let orig: extern "C" fn(*mut RawIl2CppObject, *mut RawIl2CppObject) =
            transmute(ORIG_VETERAN_APPLY);
        orig(this, trained_chara_array);
    }

    if trained_chara_array.is_null() {
        return;
    }

    log!("[Kyumaru] Hall of Fame veteran hook triggered.");

    let mut visited = HashSet::new();
    let array_data = convert_object_to_value(trained_chara_array, 0, &mut visited, &[]);

    if !array_data.is_null() {
        save_veteran_data(&array_data);
    }
}

pub extern "C" fn render_kyumaru_section(ui: *mut c_void, _userdata: *mut c_void) {
    unsafe {
        ui_heading(ui, "Kyumaru Sync");
        ui_separator(ui);

        let captured = IS_CAPTURED.load(Ordering::Relaxed);
        if captured {
            let cards = *CARD_COUNT.lock().unwrap();
            let umas = *UMA_COUNT.lock().unwrap();

            ui_colored_label(
                ui,
                80,
                220,
                100,
                255,
                &format!("Status: Ready ({} cards, {} Umas)", cards, umas),
            );
            ui_small(ui, "Saved: Documents/Kyumaru/inventory.json");

            if ui_button(ui, "Open in Browser & Sync") {
                if let Some(payload) = COMPRESSED_PAYLOAD.lock().unwrap().clone() {
                    open_browser_with_payload(&payload);
                }
            }
        } else {
            ui_colored_label(
                ui,
                255,
                180,
                50,
                255,
                "Status: Waiting for login (load/index)...",
            );
            ui_small(ui, "Please log in to the game or return to title.");
        }

        ui_separator(ui);
        ui_colored_label(
            ui,
            160,
            160,
            160,
            255,
            "Notice: Auto-updates on title/login. Normal packets bypass with 0ms lag.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn test_process_real_msgpack_packet() {
        let _ = crate::config::init_config();
        let path = "CarrotJuicerResponses/1788861969892R.msgpack";
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                File::open("../CarrotJuicerResponses/1788861969892R.msgpack")
                    .or_else(|_| File::open("kyumaru/CarrotJuicerResponses/1788861969892R.msgpack"))
                    .expect("Failed to open test msgpack file")
            }
        };
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();

        assert!(find_subsequence(&buffer, b"support_card_list"));
        process_load_index_payload(&buffer);

        let cards = *CARD_COUNT.lock().unwrap();
        let umas = *UMA_COUNT.lock().unwrap();
        assert_eq!(cards, 352);
        assert_eq!(umas, 43);
        assert!(COMPRESSED_PAYLOAD.lock().unwrap().is_some());
    }
}
