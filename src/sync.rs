use crate::config::target_url;
use crate::log;
use crate::plugin_api::show_toast;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde_json::Value;
use std::io::Write;

pub fn base64_url_encode(data: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::with_capacity((data.len() * 4 + 2) / 3);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        result.push(CHARSET[(b0 >> 2) as usize] as char);
        result.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARSET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(CHARSET[(b2 & 0x3F) as usize] as char);
        }
    }
    result
}

pub fn compress_and_encode(payload: &Value) -> Result<String, String> {
    let json_bytes = serde_json::to_vec(payload).map_err(|e| format!("JSON encode: {}", e))?;

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(&json_bytes)
        .map_err(|e| format!("Deflate write: {}", e))?;
    let compressed = encoder
        .finish()
        .map_err(|e| format!("Deflate finish: {}", e))?;

    Ok(base64_url_encode(&compressed))
}

pub fn open_browser_with_payload(payload_b64: &str) {
    let base = target_url().trim_end_matches('/');
    let target = format!("{}/#data={}", base, payload_b64);

    log!("[Sync] Opening browser with URL length: {} chars", target.len());

    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", &target])
                .spawn();
        }

        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(&target).spawn();
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            let _ = std::process::Command::new("xdg-open").arg(&target).spawn();
        }

        unsafe {
            show_toast("Browser opened! Syncing cards to AlmondEyeDB...");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::DeflateDecoder;
    use serde_json::json;
    use std::io::Read;

    fn base64_url_decode(s: &str) -> Option<Vec<u8>> {
        fn decode_char(c: u8) -> Option<u8> {
            match c {
                b'A'..=b'Z' => Some(c - b'A'),
                b'a'..=b'z' => Some(c - b'a' + 26),
                b'0'..=b'9' => Some(c - b'0' + 52),
                b'-' => Some(62),
                b'_' => Some(63),
                _ => None,
            }
        }

        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
        for chunk in bytes.chunks(4) {
            let c0 = decode_char(chunk[0])?;
            let c1 = decode_char(chunk.get(1).copied().unwrap_or(b'A'))?;
            out.push((c0 << 2) | (c1 >> 4));

            if chunk.len() > 2 {
                let c2 = decode_char(chunk[2])?;
                out.push(((c1 & 0x0F) << 4) | (c2 >> 2));

                if chunk.len() > 3 {
                    let c3 = decode_char(chunk[3])?;
                    out.push(((c2 & 0x03) << 6) | c3);
                }
            }
        }
        Some(out)
    }

    #[test]
    fn test_roundtrip_compression() {
        let sample = json!({
            "cards": {
                "30005": 1,
                "30016": 4
            },
            "umas": {
                "100601": [3, 5]
            }
        });

        let b64 = compress_and_encode(&sample).expect("compression failed");
        assert!(!b64.is_empty());

        let compressed = base64_url_decode(&b64).expect("base64 decode failed");
        let mut decoder = DeflateDecoder::new(&compressed[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).expect("decompression failed");

        let recovered: Value = serde_json::from_slice(&decompressed).expect("json parse failed");
        assert_eq!(recovered, sample);
    }

    #[test]
    fn test_carrotjuicer_sample_data_size_and_roundtrip() {
        let sample_path = "CarrotJuicer/1788861969892R.json";
        if let Ok(content) = std::fs::read_to_string(sample_path) {
            let parsed: Value = serde_json::from_str(&content).unwrap();
            let cards_arr = parsed["data"]["support_card_list"].as_array().unwrap();
            let umas_arr = parsed["data"]["card_list"].as_array().unwrap();

            let mut card_map = serde_json::Map::new();
            for c in cards_arr {
                let id = c["support_card_id"].as_i64().unwrap();
                let lb = c["limit_break_count"].as_i64().unwrap();
                card_map.insert(id.to_string(), json!(lb));
            }

            let mut uma_map = serde_json::Map::new();
            for u in umas_arr {
                let id = u["card_id"].as_i64().unwrap();
                let rarity = u["rarity"].as_i64().unwrap();
                let talent = u["talent_level"].as_i64().unwrap();
                uma_map.insert(id.to_string(), json!([rarity, talent]));
            }

            let payload = json!({
                "cards": card_map,
                "umas": uma_map,
            });

            let b64 = compress_and_encode(&payload).expect("compression failed");
            println!("CarrotJuicer sample payload B64 length: {} chars", b64.len());
            // Must fit comfortably in standard URL (< 2000 chars)
            assert!(b64.len() < 2000);

            // Verify perfect roundtrip recovery
            let compressed = base64_url_decode(&b64).expect("base64 decode failed");
            let mut decoder = DeflateDecoder::new(&compressed[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed).expect("decompression failed");
            let recovered: Value = serde_json::from_slice(&decompressed).expect("json parse failed");
            assert_eq!(recovered, payload);
        }
    }
}

